use chrono::{DateTime, NaiveDateTime, Utc};
use ciborium::ser::into_writer;
use ciborium::value::{Integer, Value};
use grovedb::Error;
use indexmap::IndexMap;
use itertools::Itertools;
use prettytable::{Cell, Row, Table};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_distr::num_traits::Pow;
use rocksdb::{OptimisticTransactionDB, Transaction};
use rs_drive::common;
use rs_drive::contract::types::DocumentFieldType;
use rs_drive::contract::{Contract, Document, DocumentType};
use rs_drive::drive::object_size_info::DocumentInfo::DocumentAndSerialization;
use rs_drive::drive::object_size_info::{DocumentAndContractInfo, DocumentInfo};
use rs_drive::drive::Drive;
use rs_drive::query::{DriveQuery, InternalClauses, OrderClause};
use rustyline::config::Configurer;
use rustyline::Editor;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::default::Default;
use std::io::Write;
use std::time::SystemTime;
use tempdir::TempDir;

pub const DASH_PRICE: f64 = 127.0;

fn print_contract_format(contract: &Contract) {
    for (document_type_name, document_type) in contract.document_types.iter() {
        println!("## {}", document_type_name);
        for (property_name) in document_type.properties.keys().sorted() {
            let document_field_type = document_type.properties.get(property_name).unwrap();
            println!("#### {} : {}", property_name, document_field_type);
        }
    }
}

fn print_contract_options(_contract: &Contract) {
    println!();
    println!("#########################################################");
    println!("### You have the following options for this contract: ###");
    println!("#########################################################");
    println!();
    println!("### view / v                                                      - view contract structure");
    println!("### pop <document_type> <number>                                  - populate with random data a specific document_type"
    );
    println!(
        "### insert / i <document_type> <field_0> <field_1> .. <field_n>   - add a specific item"
    );
    println!(
        "### delete <document_type> <id>                                   - remove an item by id"
    );
    println!("### all <document_type> <[sortBy1,sortBy2...]> <limit>            - get all people sorted by defined fields");
    // println!(
    //     "### query <sqlQuery>                                   - sql like query on the system"
    // );
    println!("### cost <document_type_name>                                     - get the worst case scenario insertion cost"
    );
    println!();
}

pub fn populate_with_documents(
    documents: Vec<Document>,
    drive: &Drive,
    document_type: &DocumentType,
    contract: &Contract,
) -> Result<(i64, u64), Error> {
    let db_transaction = drive.grove.start_transaction();
    let mut storage_fee = 0;
    let mut processing_fee = 0;
    for document in documents.iter() {
        let document_cbor = document.to_cbor();
        let (s, p) = drive.add_document_for_contract(
            DocumentAndContractInfo {
                document_info: DocumentInfo::DocumentAndSerialization((
                    document,
                    document_cbor.as_slice(),
                )),
                contract,
                document_type,
                owner_id: None,
            },
            false,
            0.0,
            Some(&db_transaction),
        )?;
        storage_fee += s;
        processing_fee += p;
    }
    drive.grove.commit_transaction(db_transaction)?;
    Ok((storage_fee, processing_fee))
}

fn prompt_populate(input: String, drive: &Drive, contract: &Contract) {
    let args: Vec<&str> = input.split_whitespace().collect();
    if args.len() != 3 {
        println!("### ERROR! Two parameter should be provided");
    } else if let Some(count_str) = args.last() {
        let document_type_name = args.get(1).unwrap();
        let document_type = contract.document_type_for_name(document_type_name);
        match document_type {
            Ok(document_type) => match count_str.parse::<u32>() {
                Ok(value) => {
                    if value > 0 && value <= 10000 {
                        let documents = document_type.random_documents(value, None);
                        let start_time = SystemTime::now();
                        let (storage_fee, processing_fee) =
                            populate_with_documents(documents, drive, document_type, contract)
                                .expect("populate returned an error");
                        if let Ok(n) = SystemTime::now().duration_since(start_time) {
                            println!(
                                "Storage fee: {} ({:.2}¢)",
                                storage_fee,
                                (storage_fee as f64) * 10_f64.pow(-9) * DASH_PRICE
                            );
                            println!(
                                "Processing fee: {} ({:.2}¢)",
                                processing_fee,
                                (processing_fee as f64) * 10_f64.pow(-9) * DASH_PRICE
                            );
                            println!("Time taken: {}", n.as_secs_f64());
                        }
                    } else {
                        println!("### ERROR! Value must be between 1 and 10000");
                    }
                }
                Err(_) => {
                    println!("### ERROR! An integer was not provided for the population");
                }
            },
            Err(_) => {
                println!("### ERROR! Contract did not have that document type");
            }
        }
    }
}

fn prompt_insert(input: String, drive: &Drive, contract: &Contract) {
    let args = input.split_whitespace();
    let count = &args.count();
    if *count < 2 {
        println!(
            "### ERROR! At least 2 parameters should be provided, got {} for {}",
            *count, input
        );
    } else {
        let split: Vec<String> = input.split_whitespace().map(|s| s.to_string()).collect();
        let document_type_name = split.get(1).unwrap();
        let document_type_result = contract.document_type_for_name(document_type_name);
        match document_type_result {
            Ok(document_type) => {
                let fields_count = &document_type.properties.len();
                if *count != fields_count + 2 {
                    println!(
                        "### ERROR! Exactly {} parameters should be provided",
                        fields_count + 2
                    );
                } else {
                    let mut hashmap: HashMap<String, Value> = HashMap::new();
                    for (i, property_name) in
                        (2..=*fields_count).zip(&mut document_type.properties.keys().sorted())
                    {
                        let value = split.get(i).unwrap();
                        let property_type = document_type.properties.get(property_name).unwrap();
                        let value: Value = property_type
                            .value_from_string(value)
                            .expect("expected to get a value");
                        hashmap.insert(property_name.clone(), value);
                    }
                    let mut rng = rand::rngs::StdRng::from_entropy();
                    let id = Vec::from(rng.gen::<[u8; 32]>());
                    let owner_id = Vec::from(rng.gen::<[u8; 32]>());
                    hashmap.insert("$id".to_string(), Value::Bytes(id));
                    hashmap.insert("$ownerId".to_string(), Value::Bytes(owner_id));

                    let value = serde_json::to_value(&hashmap).expect("serialized item");
                    let document_cbor = common::value_to_cbor(
                        value,
                        Some(rs_drive::drive::defaults::PROTOCOL_VERSION),
                    );
                    let document = Document::from_cbor(document_cbor.as_slice(), None, None)
                        .expect("document should be properly deserialized");

                    let start_time = SystemTime::now();
                    let db_transaction = drive.grove.start_transaction();
                    let (storage_fee, processing_fee) = drive
                        .add_document_for_contract(
                            DocumentAndContractInfo {
                                document_info: DocumentAndSerialization((
                                    &document,
                                    &document_cbor,
                                )),
                                contract,
                                document_type,
                                owner_id: None,
                            },
                            true,
                            0f64,
                            Some(&db_transaction),
                        )
                        .expect("document should be inserted");
                    drive
                        .grove
                        .commit_transaction(db_transaction)
                        .map_err(|err| {
                            println!("### ERROR! Unable to commit transaction");
                            println!("### Info {:?}", err);
                        });
                    if let Ok(n) = SystemTime::now().duration_since(start_time) {
                        println!(
                            "Storage fee: {} ({:.2}¢)",
                            storage_fee,
                            (storage_fee as f64) * 10_f64.pow(-9) * DASH_PRICE
                        );
                        println!(
                            "Processing fee: {} ({:.2}¢)",
                            processing_fee,
                            (processing_fee as f64) * 10_f64.pow(-9) * DASH_PRICE
                        );
                        println!("Time taken: {}", n.as_secs_f64());
                    }
                }
            }
            Err(_) => {
                println!("### ERROR! Document type does not exist");
            }
        }
    }
}

fn prompt_delete(input: String, drive: &Drive, contract: &Contract) {
    let args = input.split_whitespace();
    if args.count() != 3 {
        println!("### ERROR! Two parameter should be provided");
    } else {
        let split: Vec<String> = input.split_whitespace().map(|s| s.to_string()).collect();
        let document_type_name = split.get(1).unwrap().as_str();
        let id_bs58 = split.get(2).unwrap().as_str();
        let id = bs58::decode(id_bs58).into_vec();
        if id.is_err() {
            println!("### ERROR! Could not decode id");
        }
        let id = id.unwrap();
        if drive
            .delete_document_for_contract(id.as_slice(), contract, document_type_name, None, None)
            .is_err()
        {
            println!("### ERROR! Could not delete document");
        }
    }
}
//
// fn prompt_query(input: String, drive: &Drive, contract: &Contract) {
//     let query = DriveQuery::from_sql_expr(input.as_str(), &contract).expect("should build query");
//     let results = query.execute_no_proof(&drive.grove, None);
//     if let Ok((results, _)) = results {
//         let people: Vec<Person> = results
//             .into_iter()
//             .map(|result| {
//                 let document = Document::from_cbor(result.as_slice(), None, None)
//                     .expect("we should be able to deserialize the cbor");
//                 Person::from_document(document)
//             })
//             .collect();
//         people.iter().for_each(|person| person.println());
//     } else {
//         println!("invalid query, try again");
//     }
// }

fn prompt_cost(input: String, drive: &Drive, contract: &Contract) {
    let args = input.split_whitespace();
    if args.count() != 2 {
        println!("### ERROR! Two parameter should be provided");
    } else {
        let document_type_name = input.split_whitespace().last().unwrap();
        let document_type_result = contract.document_type_for_name(document_type_name);
        match document_type_result {
            Ok(_) => {
                match drive.worst_case_fee_for_document_type_with_name(contract, document_type_name)
                {
                    Ok((storage_fee, processing_fee)) => {
                        println!("For {} document type:", document_type_name);
                        println!(
                            "Worst case storage fee: {} ({:.2}¢)",
                            storage_fee,
                            (storage_fee as f64) * 10_f64.pow(-9) * DASH_PRICE
                        );
                        println!(
                            "Worst case processing fee: {} ({:.2}¢)",
                            processing_fee,
                            (processing_fee as f64) * 10_f64.pow(-9) * DASH_PRICE
                        );
                    }
                    Err(e) => {
                        println!("### ERROR! Could not get worst case fee from contract");
                    }
                }
            }
            Err(_) => {
                println!("### ERROR! Document type does not exist");
            }
        }
    }
}

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
    let (results, _) = query
        .execute_no_proof(&drive.grove, None)
        .expect("proof should be executed");
    println!("result len: {}", results.len());
    let documents: Vec<Document> = results
        .into_iter()
        .map(|result| {
            Document::from_cbor(result.as_slice(), None, None)
                .expect("we should be able to deserialize the cbor")
        })
        .collect();
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
            let document_field_type = document_type.properties.get(key).unwrap();
            cells.push(Cell::new(
                reduced_value_string_representation(value, document_field_type).as_str(),
            ));
        }
        table.add_row(Row::new(cells));
    }

    table.printstd();
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

fn contract_rl(drive: &Drive, contract: &Contract, rl: &mut Editor<()>) -> bool {
    let readline = rl.readline("> ");
    match readline {
        Ok(input) => {
            if input.starts_with("view ") || input == "v" {
                print_contract_format(contract);
                true
            } else if input.starts_with("pop ") {
                prompt_populate(input, &drive, contract);
                true
            } else if input.starts_with("all") {
                prompt_all(input, &drive, &contract);
                true
            } else if input.starts_with("insert ") || input == "i" {
                prompt_insert(input, &drive, &contract);
                true
            } else if input.starts_with("delete ") {
                prompt_delete(input, &drive, &contract);
                true
            } else if input.starts_with("select ") {
                //prompt_query(input, &drive, &contract);
                true
            } else if input.starts_with("cost ") {
                prompt_cost(input, &drive, &contract);
                true
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

pub fn contract_loop(drive: &Drive, contract: &Contract, rl: &mut Editor<()>) -> bool {
    print_contract_options(&contract);
    contract_rl(drive, contract, rl)
}
