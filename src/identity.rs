use chrono::{DateTime, NaiveDateTime, Utc};
use ciborium::ser::into_writer;
use ciborium::value::{Integer as cborInteger, Value};
use indexmap::IndexMap;
use itertools::Itertools;
use num_integer::Integer;
use prettytable::{Cell, Row, Table};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_distr::num_traits::Pow;
use rs_drive::common;
use rs_drive::contract::types::DocumentFieldType;
use rs_drive::contract::{Contract, document::Document, DocumentType};
use rs_drive::drive::object_size_info::DocumentInfo::DocumentAndSerialization;
use rs_drive::drive::object_size_info::{DocumentAndContractInfo, DocumentInfo};
use rs_drive::drive::Drive;
use rs_drive::error::Error;
use rs_drive::query::{DriveQuery, InternalClauses, OrderClause, WhereClause, WhereOperator};
use rustyline::config::Configurer;
use rustyline::Editor;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::default::Default;
use std::io::Write;
use std::num::ParseIntError;
use std::time::SystemTime;
use rs_drive::drive::flags::StorageFlags;
use rs_drive::identity::Identity;
use tempdir::TempDir;

pub const DASH_PRICE: f64 = 100.0;

fn print_identity_options() {
    println!();
    println!("######################################################");
    println!("### You have the following options for identities: ###");
    println!("######################################################");
    println!();
    println!("### pop <number> <key_count> <option:'include_worst_case'>        - populate with random data for identities"
    );
    println!(
        "### insert / i <field_0> <field_1> .. <field_n>   - add a specific item"
    );
    println!(
        "### dryinsert <field_0> <field_1> .. <field_n>   - add a specific item"
    );
    println!(
        "### delete <id>                                   - remove an item by id"
    );
    println!("### all <limit>            - get all identities");
    println!(
        "### select <sqlQuery>                                             - sql like query on the system"
    );
    println!();
}

pub fn populate_with_identities(
    identities: Vec<Identity>,
    drive: &Drive,
    apply: bool,
) -> Result<(i64, u64), Error> {
    let storage_flags = StorageFlags { epoch: 0 };
    let db_transaction = drive.grove.start_transaction();
    let mut storage_fee = 0;
    let mut processing_fee = 0;
    for identity in identities.into_iter() {
        let (s, p) = drive.insert_new_identity(
            identity,
            storage_flags.clone(),
            false,
            apply,
            Some(&db_transaction),
        )?;
        storage_fee += s;
        processing_fee += p;
    }
    drive.grove.commit_transaction(db_transaction)?;
    Ok((storage_fee, processing_fee))
}

fn populate_many_identities(
    count: u16,
    key_count: u16,
    drive: &Drive,
    i: Option<u32>,
    export_csv: bool,
    include_worst_case: bool,
) {
    let identities = Identity::random_identities(count, key_count, None);
    if include_worst_case {
        populate_identities_with_descriptions(identities.clone(), drive, i, export_csv, false);
    }
    populate_identities_with_descriptions(identities, drive, i, export_csv, true);
}

fn print_fees(storage_fee: i64, processing_fee: u64, count: u32) {
    let cent_cost = (storage_fee as f64) * 10_f64.pow(-9) * DASH_PRICE;
    if cent_cost < 100f64 {
        if count > 1 {
            println!(
                "Storage fee: {} ({:.2}¢ | {:.2}¢ each)",
                storage_fee,
                cent_cost,
                cent_cost / (count as f64),
            );
        } else {
            println!(
                "Storage fee: {} ({:.2}¢",
                storage_fee,
                cent_cost,
            );
        }

    } else {
        if count > 1 {
            println!(
                "Storage fee: {} ({:.2}$ | {:.2}¢ each)",
                storage_fee,
                cent_cost / 100f64,
                cent_cost / (count as f64),
            );
        } else {
            println!(
                "Storage fee: {} ({:.2}$",
                storage_fee,
                cent_cost / 100f64,
            );
        }
    }

    let processing_cent_cost = (processing_fee as f64) * 10_f64.pow(-9) * DASH_PRICE;
    if count > 1 {
        println!(
            "Processing fee: {} ({:.2}¢ | {:.2}¢ each)",
            processing_fee,
            processing_cent_cost,
            processing_cent_cost / (count as f64),
        );
    } else {
        println!(
            "Processing fee: {} ({:.2}¢)",
            processing_fee,
            (processing_fee as f64) * 10_f64.pow(-9) * DASH_PRICE
        );
    }

}

fn populate_identities_with_descriptions(
    identities: Vec<Identity>,
    drive: &Drive,
    i: Option<u32>,
    export_csv: bool,
    apply: bool,
) {
    let start_time = SystemTime::now();
    let len = identities.len() as u32;
    let (storage_fee, processing_fee) =
        populate_with_identities(identities, drive, apply)
            .expect("populate returned an error");
    let mut insertion_time = 0f64;
    if let Ok(n) = SystemTime::now().duration_since(start_time) {
        insertion_time = n.as_secs_f64();
        if export_csv == false {
            if let Some(i) = i {
                println!("Step {} Apply {}", i, apply);
            }
            print_fees(storage_fee, processing_fee, len);
            println!("Time taken: {}", n.as_secs_f64());
        }
    }
    // let (queries_len, total_count, query_time) =
    //     execute_random_queries_for_document_type(drive, contract, document_type);
    // if export_csv == false {
    //     println!(
    //         "{} {} returned {} values in: {}",
    //         queries_len,
    //         if queries_len > 1 { "queries" } else { "query" },
    //         total_count,
    //         query_time
    //     );
    // } else {
    //     println!("{};{}", insertion_time, query_time);
    // }
}

fn prompt_populate(input: String, drive: &Drive) {
    let args: Vec<&str> = input.split_whitespace().collect();
    if args.len() != 3 && args.len() != 4 {
        println!("### ERROR! At max three parameters should be provided");
    } else {
        let count_str = args.get(1).unwrap();
        let key_count_str = args.get(2).unwrap();
        match count_str.parse::<u16>() {
                Ok(value) => {
                    match key_count_str.parse::<u16>() {
                        Ok(key_count) => {
                            let include_worst_case = args.get(3).map_or(false, |csv| csv.eq(&"include_worst_case"));
                            if value > 0 && value <= 10000 {
                                populate_many_identities(value, key_count, drive, None, false, include_worst_case);
                            } else {
                                println!("### ERROR! Value must be between 1 and 10000");
                            }
                        }
                        Err(_) => {
                            println!("### ERROR! An integer was not provided for the population");
                        }
                    }
                }
                Err(_) => {
                    println!("### ERROR! An integer was not provided for the population");
                }
        }
    }
}

fn prompt_bench(input: String, drive: &Drive) {
    let args: Vec<&str> = input.split_whitespace().collect();
    if args.len() != 3 && args.len() != 4 && args.len() != 5 {
        println!("### ERROR! Between two and four parameters should be provided");
    } else if let Some(count_str) = args.get(1) {
        if let Some(key_count_str) = args.get(2) {
            match count_str.parse::<u64>() {
                    Ok(value) => {
                        match key_count_str.parse::<u16>() {
                            Ok(key_value) => {
                                if value > 0 && value <= 10000000 {
                                    let step_string = args.get(3).unwrap_or(&"10000");
                                    let csv = args.get(4).map_or(false, |csv| csv.eq(&"csv"));
                                    match step_string.parse::<u64>() {
                                        Ok(step) => {
                                            let (steps_count, left) = value.div_rem(&step);
                                            for i in 0..steps_count {
                                                populate_many_identities(
                                                    step as u16,
                                                    key_value,
                                                    drive,
                                                    Some(i as u32),
                                                    csv,
                                                    false,
                                                );
                                            }
                                            populate_many_identities(
                                                left as u16,
                                                key_value,
                                                drive,
                                                Some(steps_count as u32),
                                                csv,
                                                false,
                                            );
                                        }
                                        Err(_) => {
                                            println!("### ERROR! An integer was not provided for the bench performance step");
                                        }
                                    }
                                } else {
                                    println!("### ERROR! Value must be between 1 and 10 Million");
                                }
                            }
                            Err(_) => {
                                println!("### ERROR! An integer was not provided for the population");
                            }
                        }
                    }
                    Err(_) => {
                        println!("### ERROR! An integer was not provided for the population");
                    }
            }
        }
    }
}
//
// fn prompt_populate_full(input: String, drive: &Drive, contract: &Contract) {
//     let args: Vec<&str> = input.split_whitespace().collect();
//     if args.len() != 3 {
//         println!("### ERROR! Two parameter should be provided");
//     } else if let Some(count_str) = args.last() {
//         let document_type_name = args.get(1).unwrap();
//         let document_type = contract.document_type_for_name(document_type_name);
//         match document_type {
//             Ok(document_type) => match count_str.parse::<u32>() {
//                 Ok(value) => {
//                     if value > 0 && value <= 10000 {
//                         let documents = document_type.random_filled_documents(value, None);
//                         let start_time = SystemTime::now();
//                         let (storage_fee, processing_fee) =
//                             populate_with_documents(documents, drive, document_type, contract, true)
//                                 .expect("populate returned an error");
//                         if let Ok(n) = SystemTime::now().duration_since(start_time) {
//                             print_fees(storage_fee, processing_fee, value as u32);
//                             println!("Time taken: {}", n.as_secs_f64());
//                         }
//                     } else {
//                         println!("### ERROR! Value must be between 1 and 10000");
//                     }
//                 }
//                 Err(_) => {
//                     println!("### ERROR! An integer was not provided for the population");
//                 }
//             },
//             Err(_) => {
//                 println!("### ERROR! Contract did not have that document type");
//             }
//         }
//     }
// }
//
// fn prompt_insert(input: String, drive: &Drive, contract: &Contract) {
//     let storage_flags = StorageFlags { epoch: 0 };
//     let args = input.split_whitespace();
//     let count = &args.count();
//     if *count < 2 {
//         println!(
//             "### ERROR! At least 2 parameters should be provided, got {} for {}",
//             *count, input
//         );
//     } else {
//         let split: Vec<String> = input.split_whitespace().map(|s| s.to_string()).collect();
//         let document_type_name = split.get(1).unwrap();
//         let document_type_result = contract.document_type_for_name(document_type_name);
//         match document_type_result {
//             Ok(document_type) => {
//                 let fields_count = &document_type.properties.len();
//                 if *count != fields_count + 2 {
//                     println!(
//                         "### ERROR! Exactly {} parameters should be provided",
//                         fields_count + 2
//                     );
//                 } else {
//                     let mut hashmap: HashMap<String, Value> = HashMap::new();
//                     for (i, property_name) in
//                     (2..=*fields_count).zip(&mut document_type.properties.keys().sorted())
//                     {
//                         let value = split.get(i).unwrap();
//                         let property_field = document_type.properties.get(property_name).unwrap();
//                         let value: Value = property_field.document_type
//                             .value_from_string(value)
//                             .expect("expected to get a value");
//                         hashmap.insert(property_name.clone(), value);
//                     }
//                     let mut rng = rand::rngs::StdRng::from_entropy();
//                     let id = Vec::from(rng.gen::<[u8; 32]>());
//                     let owner_id = Vec::from(rng.gen::<[u8; 32]>());
//                     hashmap.insert("$id".to_string(), Value::Bytes(id));
//                     hashmap.insert("$ownerId".to_string(), Value::Bytes(owner_id));
//
//                     let value = serde_json::to_value(&hashmap).expect("serialized item");
//                     let document_cbor = common::value_to_cbor(
//                         value,
//                         Some(rs_drive::drive::defaults::PROTOCOL_VERSION),
//                     );
//                     let document = Document::from_cbor(document_cbor.as_slice(), None, None)
//                         .expect("document should be properly deserialized");
//
//                     let start_time = SystemTime::now();
//                     let db_transaction = drive.grove.start_transaction();
//                     let (storage_fee, processing_fee) = drive
//                         .add_document_for_contract(
//                             DocumentAndContractInfo {
//                                 document_info: DocumentAndSerialization((
//                                     &document,
//                                     &document_cbor,
//                                     &storage_flags,
//                                 )),
//                                 contract,
//                                 document_type,
//                                 owner_id: None,
//                             },
//                             true,
//                             0f64,
//                             true,
//                             Some(&db_transaction),
//                         )
//                         .expect("document should be inserted");
//                     drive
//                         .grove
//                         .commit_transaction(db_transaction)
//                         .map_err(|err| {
//                             println!("### ERROR! Unable to commit transaction");
//                             println!("### Info {:?}", err);
//                         })
//                         .expect("expected to commit transaction");
//                     if let Ok(n) = SystemTime::now().duration_since(start_time) {
//                         print_fees(storage_fee, processing_fee, 1);
//                         println!("Time taken: {}", n.as_secs_f64());
//                     }
//                 }
//             }
//             Err(_) => {
//                 println!("### ERROR! Document type does not exist");
//             }
//         }
//     }
// }
//
// fn prompt_delete(input: String, drive: &Drive, contract: &Contract) {
//     let args = input.split_whitespace();
//     if args.count() != 3 {
//         println!("### ERROR! Two parameter should be provided");
//     } else {
//         let split: Vec<String> = input.split_whitespace().map(|s| s.to_string()).collect();
//         let document_type_name = split.get(1).unwrap().as_str();
//         let id_bs58 = split.get(2).unwrap().as_str();
//         let id = bs58::decode(id_bs58).into_vec();
//         if id.is_err() {
//             println!("### ERROR! Could not decode id");
//         }
//         let id = id.unwrap();
//         if drive
//             .delete_document_for_contract(id.as_slice(), contract, document_type_name, None, true, None)
//             .is_err()
//         {
//             println!("### ERROR! Could not delete document");
//         }
//     }
// }
//
// fn prompt_query(input: String, drive: &Drive, contract: &Contract) {
//     let query = DriveQuery::from_sql_expr(input.as_str(), &contract).expect("should build query");
//     let results = query.execute_no_proof(&drive, None);
//     if let Ok((results, _, processing_fee)) = results {
//         let documents: Vec<Document> = results
//             .into_iter()
//             .map(|result| {
//                 Document::from_cbor(result.as_slice(), None, None)
//                     .expect("we should be able to deserialize the cbor")
//             })
//             .collect();
//         println!("processing fee is {}", processing_fee);
//         print_results(&query.document_type, documents);
//     } else {
//         println!("invalid query, try again");
//     }
// }
//
// fn prompt_cost(input: String, drive: &Drive, contract: &Contract) {
//     let args = input.split_whitespace();
//     if args.count() != 2 {
//         println!("### ERROR! Two parameter should be provided");
//     } else {
//         let document_type_name = input.split_whitespace().last().unwrap();
//         let document_type_result = contract.document_type_for_name(document_type_name);
//         match document_type_result {
//             Ok(_) => {
//                 match drive.worst_case_fee_for_document_type_with_name(contract, document_type_name)
//                 {
//                     Ok((storage_fee, processing_fee)) => {
//                         println!("For {} document type:", document_type_name);
//                         println!(
//                             "Worst case storage fee: {} ({:.2}¢)",
//                             storage_fee,
//                             (storage_fee as f64) * 10_f64.pow(-9) * DASH_PRICE
//                         );
//                         println!(
//                             "Worst case processing fee: {} ({:.2}¢)",
//                             processing_fee,
//                             (processing_fee as f64) * 10_f64.pow(-9) * DASH_PRICE
//                         );
//                     }
//                     Err(e) => {
//                         println!("### ERROR! Could not get worst case fee from contract");
//                     }
//                 }
//             }
//             Err(_) => {
//                 println!("### ERROR! Document type does not exist");
//             }
//         }
//     }
// }

fn reduced_value_string_representation(value: &Value, field_type: &DocumentFieldType) -> String {
    match value {
        Value::Integer(integer) => {
            let i: i128 = integer.clone().try_into().unwrap();
            format!("{}", i)
        }
        Value::Bytes(bytes) => hex::encode(bytes),
        Value::Float(float) => {
            match field_type {
                DocumentFieldType::Date => {
                    // Convert the timestamp string into an i64
                    let timestamp = float.floor() as i64;

                    let nano_seconds = (float * 1000.0) as u64 - (timestamp as u64 * 1000);

                    // Create a NaiveDateTime from the timestamp
                    let naive = NaiveDateTime::from_timestamp_opt(timestamp, nano_seconds as u32);

                    match naive {
                        None => {
                            format!("{}", float)
                        }
                        Some(naive) => {
                            // Create a normal DateTime from the NaiveDateTime
                            let datetime: DateTime<Utc> = DateTime::from_utc(naive, Utc);

                            // Format the datetime how you want
                            let newdate = datetime.format("%Y-%m-%d %H:%M:%S");

                            format!("{}", newdate)
                        }
                    }
                }
                _ => {
                    format!("{}", float)
                }
            }
        }
        Value::Text(text) => {
            let len = text.len();
            if len > 20 {
                let first_text = text.split_at(20).0.to_string();
                format!("{}[...({})]", first_text, len)
            } else {
                text.clone()
            }
        }
        Value::Bool(b) => {
            format!("{}", b)
        }
        Value::Null => "None".to_string(),
        Value::Tag(_, _) => "Tag".to_string(),
        Value::Array(_) => "Array".to_string(),
        Value::Map(_) => "Map".to_string(),
        _ => "".to_string(),
    }
}

fn table_for_document_type(document_type: &DocumentType) -> Table {
    let mut cells: Vec<Cell> = vec![Cell::new("$id"), Cell::new("$owner")];
    for (key, field_type) in document_type.properties.iter() {
        cells.push(Cell::new(key.as_str()));
    }

    let mut table = Table::new();
    table.add_row(Row::new(cells));
    table
}

fn print_results(document_type: &DocumentType, documents: Vec<Document>) {
    let mut table = table_for_document_type(document_type);
    for document in documents.iter() {
        let mut cells: Vec<Cell> = vec![
            Cell::new(bs58::encode(document.id.as_slice()).into_string().as_str()),
            Cell::new(
                bs58::encode(document.owner_id.as_slice())
                    .into_string()
                    .as_str(),
            ),
        ];
        for (key, value) in document.properties.iter() {
            let document_field = document_type.properties.get(key).unwrap();
            cells.push(Cell::new(
                reduced_value_string_representation(value, &document_field.document_type).as_str(),
            ));
        }
        table.add_row(Row::new(cells));
    }

    table.printstd();
}

fn all(
    order_by_strings: Vec<String>,
    limit: u16,
    drive: &Drive,
    contract: &Contract,
    document_type_name: &str,
) {
    let order_by: IndexMap<String, OrderClause> = order_by_strings
        .iter()
        .map(|field| {
            let field_string = String::from(field);
            (
                field_string.clone(),
                OrderClause {
                    field: field_string,
                    ascending: true,
                },
            )
        })
        .collect::<IndexMap<String, OrderClause>>();
    let document_type = contract
        .document_type_for_name(document_type_name)
        .expect("contract should have a person document type");
    let query = DriveQuery {
        contract,
        document_type,
        internal_clauses: InternalClauses::default(),
        offset: 0,
        limit,
        order_by,
        start_at: None,
        start_at_included: false,
        block_time: None,
    };
    let (results, _, processing_fee) = query
        .execute_no_proof(&drive, None)
        .expect("proof should be executed");
    println!("result len: {}", results.len());
    let documents: Vec<Document> = results
        .into_iter()
        .map(|result| {
            Document::from_cbor(result.as_slice(), None, None)
                .expect("we should be able to deserialize the cbor")
        })
        .collect();
    println!("processing fee is {}", processing_fee);
    print_results(&document_type, documents);
}

fn prompt_all(input: String, drive: &Drive, contract: &Contract) {
    let args = input.split_whitespace();
    let count = args.count();
    if count > 4 {
        println!("### ERROR! At max three parameters should be provided");
    } else if count < 2 {
        println!("### ERROR! At least one parameter for the document type name should be provided");
    } else {
        let split: Vec<String> = input.split_whitespace().map(|s| s.to_string()).collect();
        let document_type_name = split.get(1).unwrap();
        let arg0 = split.get(2);
        let arg1 = split.get(3);
        let (order_by_str_option, limit_str_option) = match arg1 {
            None => match arg0 {
                None => (None, None),
                Some(value) => {
                    if value.starts_with('[') {
                        (arg0, None)
                    } else {
                        (None, arg0)
                    }
                }
            },
            Some(_) => (arg0, arg1),
        };
        let mut limit = 10000;
        if let Some(limit_str) = limit_str_option {
            match limit_str.parse::<u16>() {
                Ok(value) => {
                    if value > 0 && value <= 10000 {
                        limit = value
                    } else {
                        println!("### ERROR! Limit must be between 1 and 10000");
                    }
                }
                Err(_) => {
                    println!("### ERROR! Limit was not an integer");
                }
            }
        }
        let mut order_by: Vec<String> = vec![];
        if let Some(order_by_string) = order_by_str_option {
            let order_by_str = order_by_string.as_str();
            let mut chars = order_by_str.chars();
            chars.next();
            chars.next_back();
            order_by = chars.as_str().split(',').map(|s| s.to_string()).collect();
        }
        all(order_by, limit, drive, contract, document_type_name);
    }
}

fn identity_rl(drive: &Drive, rl: &mut Editor<()>) -> bool {
    let readline = rl.readline("> ");
    match readline {
        Ok(input) => {
            if input.starts_with("view ") || input == "v" {
                //print_contract_format(contract);
                true
            } else if input.starts_with("pop ") {
                prompt_populate(input, &drive);
                true
            // } else if input.starts_with("popfull ") || input.starts_with("pf ") {
            //     prompt_populate_full(input, &drive, contract);
            //     true
            // } else if input.starts_with("benchpop ") || input.starts_with("bp ") {
            //     prompt_bench(input, &drive, contract);
            //     true
            // } else if input.starts_with("all") {
            //     prompt_all(input, &drive, &contract);
            //     true
            // } else if input.starts_with("insert ") || input.starts_with("i ") {
            //     prompt_insert(input, &drive, &contract);
            //     true
            // } else if input.starts_with("delete ") {
            //     prompt_delete(input, &drive, &contract);
            //     true
            // } else if input.starts_with("select ") {
            //     prompt_query(input, &drive, &contract);
            //     true
            // } else if input.starts_with("cost ") {
            //     prompt_cost(input, &drive, &contract);
            //     true
            } else if input == "exit" {
                false
            } else {
                true
            }
        }
        Err(_) => {
            println!("no input, try again");
            true
        }
    }
}

pub fn identity_loop(drive: &Drive, rl: &mut Editor<()>) -> bool {
    print_identity_options();
    identity_rl(drive, rl)
}
