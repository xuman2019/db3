//
// lib.rs
// Copyright (C) 2022 db3.network Author imotai <codego.me@gmail.com>
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

use db3_base::{get_address_from_pk, strings};
use db3_proto::db3_account_proto::Account;
use db3_proto::db3_base_proto::{ChainId, ChainRole, UnitType, Units};
use db3_proto::db3_mutation_proto::{KvPair, Mutation, MutationAction};
use db3_sdk::mutation_sdk::MutationSDK;
use db3_sdk::store_sdk::StoreSDK;
use fastcrypto::secp256k1::Secp256k1KeyPair;
use fastcrypto::traits::EncodeDecodeBase64;
use fastcrypto::traits::KeyPair;
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
#[macro_use]
extern crate prettytable;
use prettytable::{format, Table};

const HELP: &str = r#"the help of db3 command
help    show all command
put     write pairs of key and value to db3 e.g. put ns1 key1 value1 key2 values
del     delete key from db3                 e.g. del ns1 key1 key2
get     get value from db3                  e.g. get ns1 key1 key2
range   get a range from db3                e.g. range ns1 start_key end_key
account get balance of current account
blocks  get latest blocks
"#;

fn current_seconds() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(n) => n.as_secs(),
        Err(_) => 0,
    }
}

pub fn get_key_pair(warning: bool) -> std::io::Result<Secp256k1KeyPair> {
    if warning {
        println!("WARNING, db3 will generate private key and save it to ~/.db3/key");
    }
    let user_dir: &str = "~/.db3";
    let user_key: &str = "~/.db3/key";
    std::fs::create_dir_all(user_dir)?;
    if Path::new("~/.db3/key").exists() {
        let b64_str = std::fs::read_to_string(user_key)?;
        let key_pair = Secp256k1KeyPair::decode_base64(b64_str.as_str()).unwrap();
        let addr = get_address_from_pk(&key_pair.public().pubkey);
        if warning {
            println!("restore the key with addr {:?}", addr);
        }
        Ok(key_pair)
    } else {
        let mut rng = StdRng::from_seed([0; 32]);
        let kp = Secp256k1KeyPair::generate(&mut rng);
        let addr = get_address_from_pk(&kp.public().pubkey);
        let b64_str = kp.encode_base64();
        let mut f = File::create(user_key)?;
        f.write_all(b64_str.as_bytes())?;
        f.sync_all()?;
        if warning {
            println!("create new key with addr {:?}", addr);
        }
        Ok(kp)
    }
}

fn show_account(account: &Account) {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.add_row(row![
        "total bills",
        "storage used",
        "mutation",
        "querys",
        "credits"
    ]);
    let inner_account = account.clone();
    let bills = inner_account.total_bills;
    let credits = inner_account.credits;
    table.add_row(row![
        strings::units_to_readable_num_str(&bills.unwrap()),
        strings::bytes_to_readable_num_str(account.total_storage_in_bytes),
        account.total_mutation_count,
        account.total_query_session_count,
        strings::units_to_readable_num_str(&credits.unwrap())
    ]);
    table.printstd();
}

pub async fn process_cmd(sdk: &MutationSDK, store_sdk: &StoreSDK, cmd: &str) {
    let parts: Vec<&str> = cmd.split(" ").collect();
    if parts.len() < 1 {
        println!("{}", HELP);
        return;
    }
    let cmd = parts[0];
    match cmd {
        "help" => {
            println!("{}", HELP);
            return;
        }
        "account" => {
            let kp = get_key_pair(false).unwrap();
            let addr = get_address_from_pk(&kp.public().pubkey);
            let account = store_sdk.get_account(&addr).await.unwrap();
            show_account(&account);
            return;
        }
        "range" | "blocks" => {
            println!("to be provided");
            return;
        }
        _ => {}
    }
    if parts.len() < 3 {
        println!("no enough command, e.g. put n1 k1 v1 k2 v2 k3 v3");
        return;
    }

    let ns = parts[1];
    let mut pairs: Vec<KvPair> = Vec::new();
    match cmd {
        "get" => {
            let mut keys: Vec<Vec<u8>> = Vec::new();
            for i in 2..parts.len() {
                keys.push(parts[i].as_bytes().to_vec());
            }
            if let Ok(Some(values)) = store_sdk.batch_get(ns.as_bytes(), keys).await {
                for kv in values.values {
                    println!(
                        "{} -> {}",
                        std::str::from_utf8(kv.key.as_ref()).unwrap(),
                        std::str::from_utf8(kv.value.as_ref()).unwrap()
                    );
                }
            } else {
                println!("empty set");
            }
            return;
        }
        "put" => {
            if parts.len() < 4 {
                println!("no enough command, e.g. put n1 k1 v1 k2 v2 k3 v3");
                return;
            }
            for i in 1..parts.len() / 2 {
                pairs.push(KvPair {
                    key: parts[i * 2].as_bytes().to_vec(),
                    value: parts[i * 2 + 1].as_bytes().to_vec(),
                    action: MutationAction::InsertKv.into(),
                });
            }
        }
        "del" => {
            for i in 2..parts.len() {
                pairs.push(KvPair {
                    key: parts[i].as_bytes().to_vec(),
                    value: vec![],
                    action: MutationAction::DeleteKv.into(),
                });
            }
        }
        _ => todo!(),
    }
    let mutation = Mutation {
        ns: ns.as_bytes().to_vec(),
        kv_pairs: pairs.to_owned(),
        nonce: current_seconds(),
        gas_price: Some(Units {
            utype: UnitType::Tai.into(),
            amount: 100,
        }),
        gas: 100,
        chain_id: ChainId::DevNet.into(),
        chain_role: ChainRole::StorageShardChain.into(),
    };

    if let Ok(_) = sdk.submit_mutation(&mutation).await {
        println!("submit mutation to mempool done!");
    } else {
        println!("fail to submit mutation to mempool");
    }
}