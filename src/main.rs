mod contract;
pub mod person;
mod identity;

use crate::contract::contract_loop;
use crate::person::person_loop;
use crate::ContractType::{DPNSContract, DashPayContract, OtherContract, PersonContract, Identity};
use rand::{Rng, SeedableRng};
use rs_drive::common;
use rs_drive::contract::{Contract, document::Document, DocumentType};
use rs_drive::drive::Drive;
use rs_drive::query::{DriveQuery, InternalClauses, OrderClause};
use rustyline::config::Configurer;
use rustyline::Editor;
use std::collections::HashMap;
use std::default::Default;
use std::fs;
use std::path::Path;
use rs_drive::error::Error;
use tempdir::TempDir;
use crate::identity::identity_loop;

pub const LAST_CONTRACT_PATH: &str = "last_contract_path";

struct Explorer {
    config: HashMap<String, String>,
}

impl Explorer {
    fn load_config() -> Self {
        let path = Path::new("explorer.config");

        let read_result = fs::read(path);
        let config = match read_result {
            Ok(data) => bincode::deserialize(&data).expect("config file is corrupted"),
            Err(_) => HashMap::new(),
        };
        Explorer { config }
    }

    fn save_config(&self) {
        let config =
            bincode::serialize(&self.config).expect("unable to serialize root leaves data");
        let path = Path::new("explorer.config");

        fs::write(path, config).unwrap();
    }

    fn load_last_contract(&self, drive: &Drive) -> Option<Contract> {
        let last_contract_path = self.config.get(LAST_CONTRACT_PATH)?;
        let db_transaction = drive.grove.start_transaction();

        let mut rng = rand::rngs::StdRng::from_entropy();
        let contract_id = rng.gen::<[u8; 32]>();
        let contract = common::setup_contract(
            &drive,
            last_contract_path,
            Some(contract_id),
            Some(&db_transaction),
        );
        drive.grove.commit_transaction(db_transaction).ok();
        Some(contract)
    }

    fn load_contract(&mut self, drive: &Drive, contract_path: &str) -> Result<Contract, Error> {
        let db_transaction = drive.grove.start_transaction();

        let mut rng = rand::rngs::StdRng::from_entropy();
        let contract_id = rng.gen::<[u8; 32]>();
        let contract = common::setup_contract(
            &drive,
            contract_path,
            Some(contract_id),
            Some(&db_transaction),
        );
        drive.commit_transaction(db_transaction)?;
        self.config
            .insert(LAST_CONTRACT_PATH.to_string(), contract_path.to_string());
        self.save_config();
        Ok(contract)
    }

    fn load_person_contract(&mut self, drive: &Drive) -> Result<Contract, Error> {
        self.load_contract(
            drive,
            "src/supporting_files/contract/family/family-contract.json",
        )
    }

    fn load_dashpay_contract(&mut self, drive: &Drive) -> Result<Contract, Error> {
        self.load_contract(drive, "src/supporting_files/contract/dashpay-contract.json")
    }

    fn load_dpns_contract(&mut self, drive: &Drive) -> Result<Contract, Error> {
        self.load_contract(drive, "src/supporting_files/contract/dpns-contract.json")
    }

    fn base_rl(
        &mut self,
        drive: &Drive,
        rl: &mut Editor<()>,
    ) -> (bool, Option<(ContractType, Option<Contract>)>) {
        let readline = rl.readline("> ");
        match readline {
            Ok(input) => {
                if input.eq("identity") || input.eq("i") {
                    (
                        true,
                        Some((
                            Identity,
                            Some(self.load_person_contract(drive)
                                .expect("expected to load person contract")),
                        )),
                    )
                } else if input.eq("person") || input.eq("p") {
                    (
                        true,
                        Some((
                            PersonContract,
                            Some(self.load_person_contract(drive)
                                .expect("expected to load person contract")),
                        )),
                    )
                } else if input.eq("dashpay") || input.eq("dp") {
                    (
                        true,
                        Some((
                            DashPayContract,
                            Some(self.load_dashpay_contract(drive)
                                .expect("expected to load person contract")),
                        )),
                    )
                } else if input.eq("dpns") {
                    (
                        true,
                        Some((
                            DPNSContract,
                            Some(self.load_dpns_contract(drive)
                                .expect("expected to load person contract")),
                        )),
                    )
                } else if input.starts_with("l ") || input.starts_with("load ") {
                    match prompt_load_contract(input) {
                        None => (true, None),
                        Some(contract_path) => {
                            match self.load_contract(drive, contract_path.as_str()) {
                                Ok(contract) => (true, Some((OtherContract, Some(contract)))),
                                Err(_) => {
                                    println!("### ERROR! Issue loading contract");
                                    (true, None)
                                }
                            }
                        }
                    }
                } else if input == "ll" || input == "loadlast" {
                    match self.load_last_contract(drive) {
                        Some(contract) => (true, Some((OtherContract, Some(contract)))),
                        None => (true, None),
                    }
                } else if input == "exit" {
                    (false, None)
                } else {
                    (true, None)
                }
            }
            Err(_) => {
                println!("no input, try again");
                (true, None)
            }
        }
    }

    fn base_loop(
        &mut self,
        drive: &Drive,
        rl: &mut Editor<()>,
    ) -> (bool, Option<(ContractType, Option<Contract>)>) {
        print_base_options();
        self.base_rl(drive, rl)
    }
}

enum ContractType {
    Identity, //Not really a contract
    PersonContract,
    DashPayContract,
    DPNSContract,
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
    println!("### identity / i                    - enter identity explorer");
    println!("### person / p                      - load the person contract");
    println!("### dashpay                         - load the dashpay contract");
    println!("### dpns                            - load the dpns contract");
    println!("### load / l <contract file path>   - load a specific contract");
    println!("### loadlast / ll                   - load the last loaded contract");
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

fn main() {
    print_welcome();
    // setup code
    let tmp_dir = TempDir::new("family").unwrap();
    let drive: Drive = Drive::open(&tmp_dir).expect("expected to open Drive successfully");

    drive.create_root_tree(None);

    let mut rl = rustyline::Editor::<()>::new();
    rl.set_auto_add_history(true);

    let mut current_contract: Option<(ContractType, Option<Contract>)> = None;

    let mut explorer = Explorer::load_config();

    loop {
        if current_contract.is_some() {
            match &current_contract {
                None => {}
                Some((contract_type, contract_option)) => match contract_type {
                    Identity => {
                        if !identity_loop(&drive, &mut rl) {
                            current_contract = None;
                        }
                    }
                    PersonContract => {
                        if !person_loop(&drive, contract_option.as_ref().unwrap(), &mut rl) {
                            current_contract = None;
                        }
                    }
                    _ => {
                        if !contract_loop(&drive, contract_option.as_ref().unwrap(), &mut rl) {
                            current_contract = None;
                        }
                    }
                },
            }
        } else {
            let base_result = explorer.base_loop(&drive, &mut rl);
            match base_result.0 {
                true => {
                    current_contract = base_result.1;
                }
                false => break,
            }
        }
    }
}
