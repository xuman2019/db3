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

use clap::*;

use crate::keystore::KeyStore;
use db3_crypto::id::{AccountId, DbId, TxId};
use db3_proto::db3_base_proto::{BroadcastMeta, ChainId, ChainRole};
use db3_proto::db3_database_proto::{Database, Index};
use db3_proto::db3_mutation_proto::{CollectionMutation, DatabaseAction, DatabaseMutation};
use db3_sdk::{mutation_sdk::MutationSDK, store_sdk::StoreSDK};
use prettytable::{format, Table};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct DB3ClientContext {
    pub mutation_sdk: Option<MutationSDK>,
    pub store_sdk: Option<StoreSDK>,
}

#[derive(Debug, Parser)]
#[clap(rename_all = "kebab-case")]
pub enum DB3ClientCommand {
    /// Init the client config file
    #[clap(name = "init")]
    Init {},
    /// Create a new key
    #[clap(name = "show-key")]
    ShowKey {},
    /// Create a database
    #[clap(name = "new-db")]
    NewDB {},
    /// Show the database with an address
    #[clap(name = "show-db")]
    ShowDB {
        /// the address of database
        #[clap(long)]
        addr: String,
    },
    /// Create a new collection
    #[clap(name = "new-collection")]
    NewCollection {
        /// the address of database
        #[clap(long)]
        addr: String,
        /// the name of collection
        #[clap(long)]
        name: String,
        /// the json style config of index
        #[clap(long = "index")]
        index_list: Vec<String>,
    },
    #[clap(name = "show-collection")]
    ShowCollection {
        /// the address of database
        #[clap(long)]
        addr: String,
    },
}

impl DB3ClientCommand {
    fn current_seconds() -> u64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(n) => n.as_secs(),
            Err(_) => 0,
        }
    }

    fn show_collection(database: &Database) {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
        table.set_titles(row!["name", "index",]);
        for collection in &database.collections {
            let index_str: String = collection
                .index_list
                .iter()
                .map(|i| serde_json::to_string(&i).unwrap())
                .intersperse("\n ".to_string())
                .collect();
            table.add_row(row![collection.name, index_str]);
        }
        table.printstd();
    }

    fn show_database(database: &Database) {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
        table.set_titles(row![
            "database address",
            "sender address",
            "releated transactions",
            "collections"
        ]);
        let tx_list: String = database
            .tx
            .iter()
            .map(|tx| TxId::try_from_bytes(tx).unwrap().to_base64())
            .intersperse("\n ".to_string())
            .collect();
        let collections: String = database
            .collections
            .iter()
            .map(|c| c.name.to_string())
            .intersperse("\n ".to_string())
            .collect();
        let address_ref: &[u8] = database.address.as_ref();
        let sender_ref: &[u8] = database.sender.as_ref();
        table.add_row(row![
            DbId::try_from(address_ref).unwrap().to_hex(),
            AccountId::try_from(sender_ref).unwrap().to_hex(),
            tx_list,
            collections
        ]);
        table.printstd();
    }

    pub async fn execute(self, ctx: &mut DB3ClientContext) {
        match self {
            DB3ClientCommand::Init {} => {
                if let Ok(_) = KeyStore::recover_keypair() {
                    println!("Init key successfully!");
                }
            }

            DB3ClientCommand::ShowKey {} => {
                if let Ok(ks) = KeyStore::recover_keypair() {
                    ks.show_key();
                } else {
                    println!("no key was found, you can use init command to create a new one");
                }
            }
            DB3ClientCommand::NewCollection {
                addr,
                name,
                index_list,
            } => {
                //TODO validate the index
                let index_vec: Vec<Index> = index_list
                    .iter()
                    .map(|i| serde_json::from_str::<Index>(i.as_str()).unwrap())
                    .collect();
                let collection = CollectionMutation {
                    index: index_vec.to_owned(),
                    collection_id: name.to_string(),
                };
                //TODO check database id and collection name
                let db_id = DbId::try_from(addr.as_str()).unwrap();
                let meta = BroadcastMeta {
                    //TODO get from network
                    nonce: Self::current_seconds(),
                    //TODO use config
                    chain_id: ChainId::DevNet.into(),
                    //TODO use config
                    chain_role: ChainRole::StorageShardChain.into(),
                };
                let dm = DatabaseMutation {
                    meta: Some(meta),
                    collection_mutations: vec![collection],
                    db_address: db_id.as_ref().to_vec(),
                    action: DatabaseAction::AddCollection.into(),
                };
                if let Ok((_, tx_id)) = ctx
                    .mutation_sdk
                    .as_ref()
                    .unwrap()
                    .submit_database_mutation(&dm)
                    .await
                {
                    println!("send add collection done with tx\n{}", tx_id.to_base64());
                } else {
                    println!("fail to add collection");
                }
            }
            DB3ClientCommand::ShowCollection { addr } => {
                match ctx
                    .store_sdk
                    .as_mut()
                    .unwrap()
                    .get_database(addr.as_ref())
                    .await
                {
                    Ok(Some(database)) => {
                        Self::show_collection(&database);
                    }
                    Ok(None) => {
                        println!("no collection with target address");
                    }
                    Err(e) => {
                        println!("fail to show collections with error {e}");
                    }
                }
            }

            DB3ClientCommand::ShowDB { addr } => {
                match ctx
                    .store_sdk
                    .as_mut()
                    .unwrap()
                    .get_database(addr.as_ref())
                    .await
                {
                    Ok(Some(database)) => {
                        Self::show_database(&database);
                    }
                    Ok(None) => {
                        println!("no database with target address");
                    }
                    Err(e) => {
                        println!("fail to show database with error {e}");
                    }
                }
            }

            DB3ClientCommand::NewDB {} => {
                let meta = BroadcastMeta {
                    //TODO get from network
                    nonce: Self::current_seconds(),
                    //TODO use config
                    chain_id: ChainId::DevNet.into(),
                    //TODO use config
                    chain_role: ChainRole::StorageShardChain.into(),
                };
                let dm = DatabaseMutation {
                    meta: Some(meta),
                    collection_mutations: vec![],
                    db_address: vec![],
                    action: DatabaseAction::CreateDb.into(),
                };
                if let Ok((db_id, tx_id)) = ctx
                    .mutation_sdk
                    .as_ref()
                    .unwrap()
                    .submit_database_mutation(&dm)
                    .await
                {
                    let mut table = Table::new();
                    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                    table.set_titles(row!["database address", "transaction id"]);
                    table.add_row(row![db_id.to_hex(), tx_id.to_base64()]);
                    table.printstd();
                } else {
                    println!("fail to create database");
                }
            }
        }
    }
}
