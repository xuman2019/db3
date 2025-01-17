//
// command.rs
// Copyright (C) 2023 db3.network Author imotai <codego.me@gmail.com>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

use crate::abci_impl::{AbciImpl, NodeState};
use crate::auth_storage::AuthStorage;
use crate::context::Context;
use crate::json_rpc_impl;
use crate::node_storage::NodeStorage;
use crate::storage_node_impl::StorageNodeImpl;
use actix_cors::Cors;
use actix_web::{rt, web, App, HttpServer};
use clap::Parser;
use db3_cmd::command::{DB3ClientCommand, DB3ClientContext};
use db3_crypto::db3_signer::Db3MultiSchemeSigner;
use db3_proto::db3_node_proto::storage_node_client::StorageNodeClient;
use db3_proto::db3_node_proto::storage_node_server::StorageNodeServer;
use db3_sdk::mutation_sdk::MutationSDK;
use db3_sdk::store_sdk::StoreSDK;
use http::Uri;
use merkdb::Merk;
use std::boxed::Box;
use std::io::{stderr, stdout};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use tendermint_abci::ServerBuilder;
use tendermint_rpc::HttpClient;
use tonic::codegen::http::Method;
use tonic::transport::{ClientTlsConfig, Endpoint, Server};
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};
use tracing_subscriber::filter::LevelFilter;

const ABOUT: &str = "
██████╗ ██████╗ ██████╗ 
██╔══██╗██╔══██╗╚════██╗
██║  ██║██████╔╝ █████╔╝
██║  ██║██╔══██╗ ╚═══██╗
██████╔╝██████╔╝██████╔╝
╚═════╝ ╚═════╝ ╚═════╝ 
@db3.network🚀🚀🚀";

#[derive(Debug, Parser)]
#[clap(name = "db3")]
#[clap(about = ABOUT, long_about = None)]
pub enum DB3Command {
    /// Start db3 network
    #[clap(name = "start")]
    Start {
        /// Bind the gprc server to this .
        #[clap(long, default_value = "127.0.0.1")]
        public_host: String,
        /// The port of grpc api
        #[clap(long, default_value = "26659")]
        public_grpc_port: u16,
        #[clap(long, default_value = "26670")]
        public_json_rpc_port: u16,
        /// Bind the abci server to this port.
        #[clap(long, default_value = "26658")]
        abci_port: u16,
        /// The porf of tendemint
        #[clap(long, default_value = "26657")]
        tendermint_port: u16,
        /// The default server read buffer size, in bytes, for each incoming client
        /// connection.
        #[clap(short, long, default_value = "1048576")]
        read_buf_size: usize,
        /// Increase output logging verbosity to DEBUG level.
        #[clap(short, long)]
        verbose: bool,
        /// Suppress all output logging (overrides --verbose).
        #[clap(short, long)]
        quiet: bool,
        #[clap(short, long, default_value = "./db")]
        db_path: String,
        #[clap(long, default_value = "16")]
        db_tree_level_in_memory: u8,
        /// disable grpc-web
        #[clap(long, default_value = "false")]
        disable_grpc_web: bool,
    },

    /// Start db3 interactive console
    #[clap(name = "console")]
    Console {
        /// the url of db3 grpc api
        #[clap(long = "url", global = true, default_value = "http://127.0.0.1:26659")]
        public_grpc_url: String,
    },

    /// Run db3 client
    #[clap(name = "client")]
    Client {
        /// the url of db3 grpc api
        #[clap(long = "url", global = true, default_value = "http://127.0.0.1:26659")]
        public_grpc_url: String,
        /// the subcommand
        #[clap(subcommand)]
        cmd: Option<DB3ClientCommand>,
    },
}

impl DB3Command {
    fn build_context(public_grpc_url: &str) -> DB3ClientContext {
        let uri = public_grpc_url.parse::<Uri>().unwrap();
        let endpoint = match uri.scheme_str() == Some("https") {
            true => {
                let rpc_endpoint = Endpoint::new(public_grpc_url.to_string())
                    .unwrap()
                    .tls_config(ClientTlsConfig::new())
                    .unwrap();
                rpc_endpoint
            }
            false => {
                let rpc_endpoint = Endpoint::new(public_grpc_url.to_string()).unwrap();
                rpc_endpoint
            }
        };
        let channel = endpoint.connect_lazy();
        let node = Arc::new(StorageNodeClient::new(channel));
        if !db3_cmd::keystore::KeyStore::has_key() {
            db3_cmd::keystore::KeyStore::recover_keypair().unwrap();
        }
        let kp = db3_cmd::keystore::KeyStore::get_keypair().unwrap();
        let signer = Db3MultiSchemeSigner::new(kp);
        let mutation_sdk = MutationSDK::new(node.clone(), signer);
        let kp = db3_cmd::keystore::KeyStore::get_keypair().unwrap();
        let signer = Db3MultiSchemeSigner::new(kp);
        let store_sdk = StoreSDK::new(node, signer);
        DB3ClientContext {
            mutation_sdk: Some(mutation_sdk),
            store_sdk: Some(store_sdk),
        }
    }

    pub async fn execute(self) {
        match self {
            DB3Command::Console { public_grpc_url } => {
                let ctx = Self::build_context(public_grpc_url.as_ref());
                db3_cmd::console::start_console(ctx, &mut stdout(), &mut stderr())
                    .await
                    .unwrap();
            }
            DB3Command::Client {
                cmd,
                public_grpc_url,
            } => {
                let mut ctx = Self::build_context(public_grpc_url.as_ref());
                if let Some(c) = cmd {
                    c.execute(&mut ctx).await;
                }
            }
            DB3Command::Start {
                public_host,
                public_grpc_port,
                public_json_rpc_port,
                abci_port,
                tendermint_port,
                read_buf_size,
                verbose,
                quiet,
                db_path,
                db_tree_level_in_memory,
                disable_grpc_web,
            } => {
                let log_level = if quiet {
                    LevelFilter::OFF
                } else if verbose {
                    LevelFilter::DEBUG
                } else {
                    LevelFilter::INFO
                };
                tracing_subscriber::fmt().with_max_level(log_level).init();
                info!("{ABOUT}");
                let opts = Merk::default_db_opts();
                let merk = Merk::open_opt(&db_path, opts, db_tree_level_in_memory).unwrap();
                let node_store = Arc::new(Mutex::new(Box::pin(NodeStorage::new(
                    AuthStorage::new(merk),
                ))));
                match node_store.lock() {
                    Ok(mut store) => {
                        if store.get_auth_store().init().is_err() {
                            warn!("Fail to init auth storage!");
                            return;
                        }
                    }
                    _ => todo!(),
                }
                let (_node_state, abci_handler) =
                    Self::start_abci_service(abci_port, read_buf_size, node_store.clone());
                let tm_addr = format!("http://127.0.0.1:{tendermint_port}");
                info!("db3 json rpc server will connect to tendermint {tm_addr}");
                let client = HttpClient::new(tm_addr.as_str()).unwrap();
                let context = Context {
                    node_store: node_store.clone(),
                    client,
                };
                let json_rpc_handler = Self::start_json_rpc_service(
                    &public_host,
                    public_json_rpc_port,
                    context.clone(),
                );
                Self::start_grpc_service(&public_host, public_grpc_port, disable_grpc_web, context)
                    .await;
                let running = Arc::new(AtomicBool::new(true));
                let r = running.clone();
                ctrlc::set_handler(move || {
                    r.store(false, Ordering::SeqCst);
                })
                .expect("Error setting Ctrl-C handler");
                loop {
                    if running.load(Ordering::SeqCst) {
                        let ten_millis = Duration::from_millis(10);
                        thread::sleep(ten_millis);
                    } else {
                        info!("stop db3...");
                        abci_handler.join().unwrap();
                        json_rpc_handler.join().unwrap();
                        break;
                    }
                }
            }
        }
    }

    /// Start GRPC Service
    async fn start_grpc_service(
        public_host: &str,
        public_grpc_port: u16,
        disable_grpc_web: bool,
        context: Context,
    ) {
        let addr = format!("{public_host}:{public_grpc_port}");
        let kp = crate::node_key::get_key_pair(None).unwrap();
        let signer = Db3MultiSchemeSigner::new(kp);
        let storage_node = StorageNodeImpl::new(context, signer);
        info!("start db3 storage node on public addr {}", addr);
        if disable_grpc_web {
            Server::builder()
                .add_service(StorageNodeServer::new(storage_node))
                .serve(addr.parse().unwrap())
                .await
                .unwrap();
        } else {
            let cors_layer = CorsLayer::new()
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers(Any)
                .allow_origin(Any);
            Server::builder()
                .accept_http1(true)
                .layer(cors_layer)
                .layer(tonic_web::GrpcWebLayer::new())
                .add_service(StorageNodeServer::new(storage_node))
                .serve(addr.parse().unwrap())
                .await
                .unwrap();
        }
    }

    ///
    /// Start JSON RPC Service
    ///
    fn start_json_rpc_service(
        public_host: &str,
        public_json_rpc_port: u16,
        context: Context,
    ) -> JoinHandle<()> {
        let local_public_host = public_host.to_string();
        let addr = format!("{local_public_host}:{public_json_rpc_port}");
        info!("start json rpc server with addr {}", addr.as_str());
        let handler = thread::spawn(move || {
            rt::System::new()
                .block_on(async {
                    HttpServer::new(move || {
                        let cors = Cors::default()
                            .allow_any_origin()
                            .allow_any_method()
                            .allow_any_header()
                            .max_age(3600);
                        App::new()
                            .app_data(web::Data::new(context.clone()))
                            .wrap(cors)
                            .service(
                                web::resource("/").route(web::post().to(json_rpc_impl::rpc_router)),
                            )
                    })
                    .disable_signals()
                    .bind((local_public_host, public_json_rpc_port))
                    .unwrap()
                    .run()
                    .await
                })
                .unwrap();
        });
        handler
    }

    ///
    /// Start ABCI service
    ///
    fn start_abci_service(
        abci_port: u16,
        read_buf_size: usize,
        store: Arc<Mutex<Pin<Box<NodeStorage>>>>,
    ) -> (Arc<NodeState>, JoinHandle<()>) {
        let addr = format!("{}:{}", "127.0.0.1", abci_port);
        let abci_impl = AbciImpl::new(store);
        let node_state = abci_impl.get_node_state().clone();
        let handler = thread::spawn(move || {
            let server = ServerBuilder::new(read_buf_size).bind(addr, abci_impl);
            match server {
                Ok(s) => {
                    if let Err(e) = s.listen() {
                        warn!("fail to listen addr for error {}", e);
                    }
                }
                Err(e) => {
                    warn!("fail to bind addr for error {}", e);
                }
            }
        });
        (node_state, handler)
    }
}
