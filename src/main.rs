pub mod blockchain;
mod contract;
pub mod person;

use crate::blockchain::strategy::Strategy;
use crate::contract::contract_loop;
use crate::person::person_loop;
use crate::ContractType::{DPNSContract, DashPayContract, OtherContract, PersonContract};
use crate::ExplorerCommand::{EnterContract, SimulateBlockchain};
use crate::ExplorerScreen::{BlockchainScreen, ContractScreen, MainScreen, PersonContractScreen};
use blockchain::masternode::Masternode;
use dash_abci::abci::handlers::TenderdashAbci;
use dash_abci::abci::messages::InitChainRequest;
use dash_abci::common::helpers::setup::{
    setup_platform, setup_platform_with_initial_state_structure,
};
use dash_abci::platform::Platform;
use indexmap::IndexMap;
use intmap::IntMap;
use rand::{Rng, SeedableRng};
use rs_drive::common;
use rs_drive::contract::{document::Document, Contract, DocumentType};
use rs_drive::drive::Drive;
use rs_drive::error::Error;
use rs_drive::fee_pools::epochs::Epoch;
use rs_drive::query::{DriveQuery, InternalClauses, OrderClause};
use rustyline::config::Configurer;
use rustyline::Editor;
use std::collections::{BTreeMap, HashMap};
use std::default::Default;
use std::fs;
use std::ops::Range;
use std::path::Path;
use tempdir::TempDir;

pub const LAST_CONTRACT_PATH: &str = "last_contract_path";

#[derive(Clone, Copy, Default)]
struct Block {
    pub height: u64,
    pub time_ms: u64,
}

enum ExplorerScreen {
    MainScreen,
    BlockchainScreen,
    StrategyScreen,
    ContractScreen(ContractType, Contract),
    PersonContractScreen(Contract),
}

struct Explorer {
    screen: ExplorerScreen,
    last_block: Option<Block>,
    current_epoch: Option<Epoch>,
    masternodes: IndexMap<[u8; 32], Masternode>,
    current_execution_strategy: Option<(String, Strategy)>,
    config: HashMap<String, String>,
    contract_paths: BTreeMap<String, String>, //alias to contract path
    available_contracts: BTreeMap<String, Contract>, //alias to contract
    available_strategies: BTreeMap<String, Strategy>, //alias to strategy
}

enum ExplorerCommand {
    EnterContract(ContractType, Contract),
    SimulateBlockchain,
}

fn open_contract(drive: &Drive, contract_path: &str) -> Result<Contract, Error> {
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
    Ok(contract)
}

impl Explorer {
    fn load_all(platform: &Platform) -> Self {
        let path = Path::new("explorer.config");

        let read_result = fs::read(path);
        let config = match read_result {
            Ok(data) => bincode::deserialize(&data).expect("config file is corrupted"),
            Err(_) => HashMap::new(),
        };

        let path = Path::new("explorer.contracts");

        let read_result = fs::read(path);
        let contract_paths: BTreeMap<String, String> = match read_result {
            Ok(data) => bincode::deserialize(&data).expect("contracts file is corrupted"),
            Err(_) => BTreeMap::new(),
        };

        let available_contracts = contract_paths
            .iter()
            .filter_map(|(alias, path)| {
                open_contract(&platform.drive, path)
                    .map_or(None, |contract| Some((alias.clone(), contract)))
            })
            .collect();

        let path = Path::new("explorer.strategies");

        let read_result = fs::read(path);
        let available_strategies: BTreeMap<String, Strategy> = match read_result {
            Ok(data) => bincode::deserialize(&data).expect("contracts file is corrupted"),
            Err(_) => BTreeMap::new(),
        };

        Explorer {
            screen: MainScreen,
            last_block: None,
            current_epoch: None,
            masternodes: IndexMap::default(),
            current_execution_strategy: None,
            config,
            contract_paths,
            available_contracts,
            available_strategies,
        }
    }

    fn save_config(&self) {
        let config = bincode::serialize(&self.config).expect("unable to serialize config");
        let path = Path::new("explorer.config");

        fs::write(path, config).unwrap();
    }

    fn save_available_contracts(&self) {
        let contracts =
            bincode::serialize(&self.contract_paths).expect("unable to serialize contract paths");
        let path = Path::new("explorer.contracts");

        fs::write(path, contracts).unwrap();
    }

    fn save_available_strategies(&self) {
        let strategies =
            bincode::serialize(&self.available_strategies).expect("unable to serialize strategies");
        let path = Path::new("explorer.strategies");

        fs::write(path, strategies).unwrap();
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
        drive
            .grove
            .commit_transaction(db_transaction)
            .unwrap()
            .expect("expected to commit transaction");
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

    fn base_rl(&mut self, drive: &Drive, rl: &mut Editor<()>) -> (bool, Option<ExplorerCommand>) {
        let readline = rl.readline("> ");
        match readline {
            Ok(input) => {
                if input.eq("person") || input.eq("p") {
                    (
                        true,
                        Some(EnterContract(
                            PersonContract,
                            self.load_person_contract(drive)
                                .expect("expected to load person contract"),
                        )),
                    )
                } else if input.eq("dashpay") || input.eq("dp") {
                    (
                        true,
                        Some(EnterContract(
                            DashPayContract,
                            self.load_dashpay_contract(drive)
                                .expect("expected to load person contract"),
                        )),
                    )
                } else if input.eq("dpns") {
                    (
                        true,
                        Some(EnterContract(
                            DPNSContract,
                            self.load_dpns_contract(drive)
                                .expect("expected to load person contract"),
                        )),
                    )
                } else if input.starts_with("l ") || input.starts_with("load ") {
                    match prompt_load_contract(input) {
                        None => (true, None),
                        Some(contract_path) => {
                            match self.load_contract(drive, contract_path.as_str()) {
                                Ok(contract) => {
                                    (true, Some(EnterContract(OtherContract, contract)))
                                }
                                Err(_) => {
                                    println!("### ERROR! Issue loading contract");
                                    (true, None)
                                }
                            }
                        }
                    }
                } else if input == "ll" || input == "loadlast" {
                    match self.load_last_contract(drive) {
                        Some(contract) => (true, Some(EnterContract(OtherContract, contract))),
                        None => (true, None),
                    }
                } else if input == "b" || input == "blockchain" {
                    (true, Some(SimulateBlockchain))
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

    fn base_loop(&mut self, drive: &Drive, rl: &mut Editor<()>) -> (bool, Option<ExplorerCommand>) {
        print_base_options();
        self.base_rl(drive, rl)
    }
}

enum ContractType {
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
    println!("### blockchain / b                  - simulate blockchain execution");
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
    let platform = setup_platform();

    platform
        .init_chain(InitChainRequest {}, None)
        .expect("expected to init chain");

    let mut rl = rustyline::Editor::<()>::new();
    rl.set_auto_add_history(true);

    let mut explorer = Explorer::load_all(&platform);

    let mut testing_blockchain = false;

    loop {
        match &explorer.screen {
            MainScreen => {
                let base_result = explorer.base_loop(&platform.drive, &mut rl);
                match base_result.0 {
                    true => match base_result.1 {
                        None => {}
                        Some(command) => match command {
                            EnterContract(contract_type, contract) => {
                                explorer.screen = ContractScreen(contract_type, contract);
                            }
                            SimulateBlockchain => {
                                explorer.screen = BlockchainScreen;
                                testing_blockchain = true;
                            }
                        },
                    },
                    false => break, //exit from app
                }
            }
            BlockchainScreen => {
                explorer.screen = explorer.blockchain_loop(&platform, &mut rl);
            }
            StrategyScreen => {
                explorer.screen = explorer.strategy_loop(&platform, &mut rl);
            }
            ContractScreen(contract_type, contract) => {
                if !contract_loop(&platform.drive, contract, &mut rl) {
                    explorer.screen = MainScreen;
                }
            }
            PersonContractScreen(contract) => {
                if !person_loop(&platform.drive, contract, &mut rl) {
                    explorer.screen = MainScreen;
                }
            }
        }
    }
}
