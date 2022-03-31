pub mod person;

use std::collections::HashMap;
use std::default::Default;
use grovedb::Error;
use rocksdb::{OptimisticTransactionDB, Transaction};
use rs_drive::common;
use rs_drive::contract::{Contract, Document, DocumentType};
use rs_drive::drive::Drive;
use rs_drive::query::{DriveQuery, InternalClauses, OrderClause};
use rustyline::config::Configurer;
use rustyline::Editor;
use tempdir::TempDir;
use crate::ContractType::{OtherContract, PersonContract};
use crate::person::person_loop;

enum ContractType {
    PersonContract,
    OtherContract,
}

fn print_welcome() {
    println!();
    println!();
    println!("                ##########################################");
    println!("                ##########################################");
    println!("                ###### Welcome to rs-drive explorer ######");
    println!("                ##########################################");
    println!("                ##########################################");
    println!();
    println!();
}

fn print_base_options() {
    println!();
    println!("########################################");
    println!("### You have the following options : ###");
    println!("########################################");
    println!();
    println!(
        "### person / p - load the person contract"
    );
    println!("### load / l <contract file path>   - load a specific contract");
    println!();
}

fn prompt_load_contract(input: String) -> Option<String> {
    let args = input.split_whitespace();
    if args.count() != 2 {
        println!("### ERROR! Two parameter should be provided");
        None
    } else {
        input.split_whitespace().last().map(|a| a.to_string())
    }
}


fn base_rl(drive: &Drive, mut rl: &mut Editor<()>) -> (bool, Option<(ContractType, Contract)>) {
    let readline = rl.readline("> ");
    match readline {
        Ok(input) => {
            if input.eq("person") || input.eq("p") {
                (true, Some((PersonContract, load_person_contract(drive).expect("expected to load person contract"))))
            } else if input.starts_with("l ") || input.starts_with("load ") {
                match prompt_load_contract(input) {
                    None => (true, None),
                    Some(contract_path) => {
                        match load_contract(drive, contract_path.as_str()) {
                            Ok(contract) => {
                                (true, Some((OtherContract, contract)))
                            }
                            Err(_) => {
                                (true, None)
                            }
                        }
                    }
                }
            } else if input == "exit" {
                (false, None)
            } else {
                (true, None)
            }
        },
        Err(_) => {
            println!("no input, try again");
            (true, None)
        },
    }
}

fn base_loop(drive: &Drive, mut rl: &mut Editor<()>) -> (bool, Option<(ContractType, Contract)>) {
    print_base_options();
    base_rl(drive, rl)
}

fn load_contract(drive: &Drive, contract_path: &str) -> Result<Contract, Error> {
    let db_transaction = drive.grove.start_transaction();

    let contract = common::setup_contract(
        &drive,
        contract_path,
        Some(&db_transaction),
    );
    drive.grove.commit_transaction(db_transaction)?;

    Ok(contract)
}

fn load_person_contract(drive: &Drive) -> Result<Contract, Error> {
    load_contract(drive, "src/supporting_files/contract/family/family-contract.json")
}

fn main() {
    print_welcome();
    // setup code
    let tmp_dir = TempDir::new("family").unwrap();
    let drive: Drive = Drive::open(&tmp_dir).expect("expected to open Drive successfully");

    drive.create_root_tree(None);

    let mut rl = rustyline::Editor::<()>::new();
    rl.set_auto_add_history(true);

    let mut current_contract : Option<(ContractType, Contract)> = None;

    loop {
        if current_contract.is_some() {
            match &current_contract {
                None => {}
                Some((contract_type, contract)) => {
                    match contract_type {
                        ContractType::PersonContract => {
                            person_loop(&drive, contract, &mut rl);
                        }
                        ContractType::OtherContract => {}
                    }
                }
            }
        } else {
            let base_result = base_loop(&drive, &mut rl);
            match base_result.0 {
                true => { current_contract = base_result.1 }
                false => { break }
            }
        }
    }
}
