use crate::ExplorerScreen::StrategyScreen;
use crate::{Block, BlockchainScreen, ContractType, Explorer, ExplorerScreen, MainScreen};
use dash_abci::abci::handlers::TenderdashAbci;
use dash_abci::abci::messages::{BlockBeginRequest, BlockEndRequest, FeesAggregate};
use dash_abci::platform::Platform;
use masternode::Masternode;
use rand::Rng;
use rs_drive::contract::{Contract, CreateRandomDocument};
use rs_drive::drive::flags::StorageFlags;
use rs_drive::drive::object_size_info::DocumentAndContractInfo;
use rs_drive::drive::object_size_info::DocumentInfo::DocumentAndSerialization;
use rs_drive::drive::Drive;
use rs_drive::fee_pools::epochs::Epoch;
use rs_drive::grovedb::Transaction;
use rustyline::Editor;

pub mod masternode;
pub mod strategy;

fn print_blockchain_options() {
    println!();
    println!("######################################################");
    println!("### You have the following options for execution : ###");
    println!("######################################################");
    println!();
    println!("### info / i                              - get info");
    println!("### add_masternodes / a <count>           - add masternodes");
    println!("### execute_blocks / e <count>            - simulate execution of <count> blocks");
    println!("### list_epochs <start_range..end_range>  - list epochs within range");
    println!("### epoch <epoch_num>                     - enter epoch information");
    println!("### strategy / s                          - enters the strategy creation section");
    println!("### strategy_loadlast / sll               - loads the last strategy into the test");
    println!();
}

impl Explorer {
    fn add_masternodes(&mut self, count: usize) {
        let mut current_count = self.masternodes.len() as u64;
        Masternode::new_random_many(count)
            .into_iter()
            .for_each(|m| {
                self.masternodes.insert(m.pro_tx_hash, m);
                current_count += 1;
            });
    }

    fn execute_current_strategy(
        &mut self,
        drive: &Drive,
        epoch_index: u16,
        block_time: f64,
        transaction: &Transaction,
    ) -> FeesAggregate {
        let mut fees_aggregate = FeesAggregate {
            processing_fees: 0,
            storage_fees: 0,
        };

        let mut rand = rand::thread_rng();
        if let Some((alias, strategy)) = &self.current_execution_strategy {
            for (op, frequency) in &strategy.operations {
                let happens_this_block = match frequency.chance_per_block {
                    None => true,
                    Some(chance) => rand.gen_bool(chance),
                };
                if happens_this_block {
                    let count = rand.gen_range(frequency.times_per_block_range.clone());
                    let documents = op.document_type.random_documents(count as u32, None);
                    let storage_flags = StorageFlags { epoch: epoch_index };
                    for document in &documents {
                        let serialization = document
                            .serialize(&op.document_type)
                            .expect("expected to serialize document");

                        let (storage_fee, processing_fee) = drive
                            .add_document_for_contract(
                                DocumentAndContractInfo {
                                    document_info: DocumentAndSerialization((
                                        document,
                                        serialization.as_slice(),
                                        &storage_flags,
                                    )),
                                    contract: &op.contract,
                                    document_type: &op.document_type,
                                    owner_id: None,
                                },
                                false,
                                block_time,
                                true,
                                Some(transaction),
                            )
                            .expect("expected to add document");

                        fees_aggregate.storage_fees += storage_fee as u64;
                        fees_aggregate.processing_fees += processing_fee;
                    }
                }
            }
        }
        fees_aggregate
    }

    fn execute_block(&mut self, block: Block, platform: &Platform) {
        let masternode = self.random_masternode();

        let previous_block_time_ms = self.last_block.map(|b| b.time_ms);

        let Block {
            height: block_height,
            time_ms: block_time_ms,
        } = block;
        let transaction = platform.drive.grove.start_transaction();

        let begin_request = BlockBeginRequest {
            block_height,
            block_time_ms,
            previous_block_time_ms,
            proposer_pro_tx_hash: masternode.pro_tx_hash,
        };

        platform
            .block_begin(begin_request, Some(&transaction))
            .expect("expected block_begin to succeed");

        let fees = self.execute_current_strategy(
            &platform.drive,
            self.current_epoch.as_ref().map_or(0, |e| e.index),
            block_time_ms as f64 / 1000.0,
            &transaction,
        );

        let response = platform
            .block_end(BlockEndRequest { fees }, Some(&transaction))
            .expect(format!("expected block_end to succeed for block {}", block.height).as_str());

        platform
            .drive
            .commit_transaction(transaction)
            .expect("expected to commit transaction");

        self.last_block = Some(block);
        if response.is_epoch_change {
            self.current_epoch = Some(Epoch::new(response.current_epoch_index));
        }
    }

    fn execute_blocks(&mut self, platform: &Platform, count: usize) {
        let current_block = self.last_block.unwrap_or(Block {
            height: 1,
            time_ms: 100,
        });

        for height in current_block.height..(current_block.height + count as u64) {
            self.execute_block(
                Block {
                    height,
                    time_ms: height * 100,
                },
                platform,
            )
        }
    }

    fn prompt_execute_blocks(&mut self, input: String, platform: &Platform) {
        let args: Vec<&str> = input.split_whitespace().collect();
        let count = args.len();
        if count > 2 {
            println!("### ERROR! At max one parameters should be provided");
        } else if count < 2 {
            println!("### ERROR! At least one parameter for the count should be provided");
        } else {
            let count_str = args.get(1).unwrap();
            match count_str.parse::<usize>() {
                Ok(value) => {
                    if value > 0 && value <= 10000 {
                        self.execute_blocks(platform, value);
                    } else {
                        println!("### ERROR! Limit must be between 1 and 10000");
                    }
                }
                Err(_) => {
                    println!("### ERROR! Limit was not an integer");
                }
            }
        }
    }

    fn prompt_add_masternodes(&mut self, input: String) {
        let args: Vec<&str> = input.split_whitespace().collect();
        let count = args.len();
        if count > 2 {
            println!("### ERROR! At max one parameters should be provided");
        } else if count < 2 {
            println!("### ERROR! At least one parameter for the count should be provided");
        } else {
            let count_str = args.get(1).unwrap();
            match count_str.parse::<usize>() {
                Ok(value) => {
                    if value > 0 && value <= 10000 {
                        self.add_masternodes(value);
                        println!("### Added {} masternodes", value);
                        println!(
                            "### Current tally is {} masternodes",
                            self.masternodes.len()
                        );
                    } else {
                        println!("### ERROR! Limit must be between 1 and 10000");
                    }
                }
                Err(_) => {
                    println!("### ERROR! Limit was not an integer");
                }
            }
        }
    }

    fn blockchain_rl(&mut self, platform: &Platform, rl: &mut Editor<()>) -> ExplorerScreen {
        let readline = rl.readline("> ");
        match readline {
            Ok(input) => {
                if input.starts_with("view ") || input == "v" {
                    BlockchainScreen
                } else if input.starts_with("add_masternodes ") || input.starts_with("a ") {
                    self.prompt_add_masternodes(input);
                    BlockchainScreen
                } else if input.starts_with("execute_blocks ") || input.starts_with("e ") {
                    self.prompt_execute_blocks(input, platform);
                    BlockchainScreen
                } else if input == "strategy" || input == "s" {
                    StrategyScreen
                } else if input == "exit" {
                    MainScreen
                } else {
                    BlockchainScreen
                }
            }
            Err(_) => {
                println!("no input, try again");
                BlockchainScreen
            }
        }
    }

    pub(crate) fn blockchain_loop(
        &mut self,
        platform: &Platform,
        rl: &mut Editor<()>,
    ) -> ExplorerScreen {
        print_blockchain_options();
        self.blockchain_rl(platform, rl)
    }
}
