use std::ops::Range;
use dash_abci::platform::Platform;
use rs_drive::contract::{Contract, DocumentType};
use rs_drive::drive::Drive;
use rs_drive::error::Error;
use rustyline::Editor;
use crate::{BlockchainScreen, Explorer, ExplorerScreen, open_contract};
use crate::contract::print_contract_format;
use crate::ExplorerScreen::StrategyScreen;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Frequency {
    times_per_block_range: Range<u16>, //insertion count when block is chosen
    chance_per_block: Option<f64>, //chance of insertion if set
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DocumentOp {
    contract: Contract,
    document_type: DocumentType,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Strategy {
    operations: Vec<(DocumentOp, Frequency)>
}


fn print_strategy_options() {
    println!();
    println!("#######################################################");
    println!("### You have the following options for a strategy : ###");
    println!("#######################################################");
    println!();
    println!("### view_all / va                                                                    - view all strategies");
    println!("### view / v                                                                         - view current strategy");
    println!("### contracts / c                                                                    - view current available contracts");
    println!("### add_contract / ac <alias> <path>                                                 - add contract to available contracts");
    println!("### add_op / a <contract> <document_type> <times_per_block_range> <chance_per_block> - add contract to strategy");
    println!("### save_strategy / s                                                                - save strategy and keep it loaded");
    println!("### load_strategy / l <name>                                                         - load strategy");
    println!("### new_strategy / n <name>                                                          - new loaded strategy");
    println!("### dup_strategy / dup <name>                                                        - duplicate strategy and load duplicate");
    println!();
}

impl Explorer {
    fn print_strategies(&self) {
        if self.available_strategies.len() == 0 {
            println!("No available strategies, create some!");
        } else {
            for (alias, _) in &self.available_strategies {
                println!("Strategy {}", alias);
            }
        }
    }

    fn print_current_strategy(&self) {
        match &self.current_execution_strategy {
            None => {
                println!("No current strategy");
            }
            Some(strategy) => {
                println!("Strategy {:?}", strategy);
            }
        }
    }

    fn load_strategy(&mut self, alias: String) {
        if self.available_strategies.len() == 0 {
            println!("No available strategies to load");
        } else {
            match self.available_strategies.get(alias.as_str()) {
                None => {
                    println!("No available strategy for '{}'", alias);
                }
                Some(strategy) => {
                    self.current_execution_strategy = Some((alias.clone(), strategy.clone()));
                    println!("Loaded strategy '{}'", alias);
                }
            }
        }
    }

    fn prompt_load_strategy(&mut self, input: String) {
        let args: Vec<&str> = input.split_whitespace().collect();
        let count = args.len();
        if count > 2 {
            println!("### ERROR! At max two parameters for loading a strategy should be provided");
        } else if count < 2 {
            println!("### ERROR! At least two parameters for loading a strategy should be provided");
        } else {
            let alias = args.get(1).unwrap();
            self.load_strategy(alias.to_string());
        }
    }

    fn new_strategy(&mut self, alias: String) {
        self.current_execution_strategy = Some((alias.clone(), Strategy { operations: vec![] }));
        println!("New strategy '{}'", alias);
    }

    fn prompt_new_strategy(&mut self, input: String) {
        let args: Vec<&str> = input.split_whitespace().collect();
        let count = args.len();
        if count > 2 {
            println!("### ERROR! At max two parameters for creating a strategy should be provided");
        } else if count < 2 {
            println!("### ERROR! At least two parameters for creating a strategy should be provided");
        } else {
            let alias = args.get(1).unwrap();
            self.new_strategy(alias.to_string());
        }
    }

    fn dup_strategy(&mut self, alias: String) {
        match &self.current_execution_strategy {
            None => {
                println!("### ERROR! No current strategy to duplicate");
            }
            Some((previous_alias, strategy)) => {
                self.available_strategies.insert(previous_alias.clone(), strategy.clone());
                self.save_available_strategies();
                self.current_execution_strategy = Some((alias.clone(), strategy.clone()));
                println!("Duplicated strategy as '{}'", alias);
            }
        }
    }

    fn prompt_dup_strategy(&mut self, input: String) {
        let args: Vec<&str> = input.split_whitespace().collect();
        let count = args.len();
        if count > 2 {
            println!("### ERROR! At max two parameters for creating a strategy should be provided");
        } else if count < 2 {
            println!("### ERROR! At least two parameters for creating a strategy should be provided");
        } else {
            let alias = args.get(1).unwrap();
            self.dup_strategy(alias.to_string());
        }
    }

    fn save_strategy(&mut self) {
        match &self.current_execution_strategy {
            None => {
                println!("### ERROR! No current strategy to save, create one first");
            }
            Some((alias, strategy)) => {
                self.available_strategies.insert(alias.clone(), strategy.clone());
                self.save_available_strategies();
                println!("Saved strategy '{}'", alias);
            }
        }
    }

    fn print_contracts(&self) {
        if self.available_contracts.len() == 0 {
            println!("No available contracts, load some!");
        } else {
            for (alias, _) in &self.available_contracts {
                println!("Contract {}", alias);
            }
        }
    }

    fn print_contracts_full(&self) {
        if self.available_contracts.len() == 0 {
            println!("No available contracts, load some!");
        } else {
            for (alias, contract) in &self.available_contracts {
                println!("Contract {}", alias);
                println!("--------------------------------------");
                print_contract_format(&contract);
                println!("--------------------------------------");
            }
        }
    }

    fn add_contract(&mut self, drive: &Drive, alias: String, path: String) {
        let contract_result = open_contract(drive, path.as_str());
        match contract_result {
            Ok(contract) => {
                self.contract_paths.insert(alias.clone(), path);
                self.available_contracts.insert(alias.clone(), contract);
                self.save_available_contracts();
                println!("### Successfully added contract {}", alias);
            }
            Err(e) => {
                println!("### ERROR! Unable to load contract {:?}", e);
            }
        }
    }

    fn prompt_add_contract(&mut self, input: String, drive: &Drive) {
        let args: Vec<&str> = input.split_whitespace().collect();
        let count = args.len();
        if count > 3 {
            println!("### ERROR! At max two parameters for adding a contract should be provided");
        } else if count < 3 {
            println!("### ERROR! At least two parameters for adding a contract should be provided");
        } else {
            let alias = args.get(1).unwrap();
            let path = args.get(2).unwrap();
            self.add_contract(drive, alias.to_string(), path.to_string());
        }
    }

    fn strategy_rl(&mut self, platform: &Platform, rl: &mut Editor<()>) -> ExplorerScreen {
        let readline = rl.readline("> ");
        match readline {
            Ok(input) => {
                if input == "view_all" || input == "va" {
                    self.print_strategies();
                    StrategyScreen
                } else if input == "view" || input == "v" {
                    self.print_current_strategy();
                    StrategyScreen
                } else if input.starts_with("load_strategy ") || input.starts_with("l ") {
                    self.prompt_load_strategy(input);
                    StrategyScreen
                } else if input.starts_with("new_strategy ") || input.starts_with("n ") {
                    self.prompt_new_strategy(input);
                    StrategyScreen
                } else if input.starts_with("dup_strategy ") || input.starts_with("dup ") {
                    self.prompt_dup_strategy(input);
                    StrategyScreen
                } else if input == "save_strategy " || input == "s" {
                    self.save_strategy();
                    StrategyScreen
                } else if input == "contracts" || input == "c" {
                    self.print_contracts();
                    StrategyScreen
                } else if input.starts_with("add_contract ") || input.starts_with("ac ") {
                    self.prompt_add_contract(input, &platform.drive);
                    StrategyScreen
                } else if input.starts_with("add_op ") || input.starts_with("a ") {
                    StrategyScreen
                } else if input == "exit" {
                    BlockchainScreen
                } else {
                    StrategyScreen
                }
            }
            Err(_) => {
                println!("no input, try again");
                StrategyScreen
            }
        }
    }

    pub(crate) fn strategy_loop(
        &mut self,
        platform: &Platform,
        rl: &mut Editor<()>,
    ) -> ExplorerScreen {
        print_strategy_options();
        self.strategy_rl(platform, rl)
    }
}
