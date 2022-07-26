use indexmap::IndexMap;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_distr::num_traits::Pow;
use rs_drive::common;
use rs_drive::contract::{document::Document, Contract, DocumentType};
use rs_drive::drive::flags::StorageFlags;
use rs_drive::drive::object_size_info::DocumentInfo::DocumentAndSerialization;
use rs_drive::drive::object_size_info::{DocumentAndContractInfo, DocumentInfo};
use rs_drive::drive::Drive;
use rs_drive::error::Error;
use rs_drive::grovedb::Transaction;
use rs_drive::query::{DriveQuery, InternalClauses, OrderClause};
use rustyline::config::Configurer;
use rustyline::Editor;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::default::Default;
use std::io::Write;
use std::time::SystemTime;
use rs_drive::dpp::data_contract::extra::DriveContractExt;
use tempdir::TempDir;

pub const DASH_PRICE: f64 = 127.0;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Person {
    #[serde(rename = "$id")]
    id: Vec<u8>,
    #[serde(rename = "$ownerId")]
    owner_id: Vec<u8>,
    first_name: String,
    middle_name: String,
    last_name: String,
    age: u8,
}

impl Person {
    fn new_with_random_ids(first_name: &str, middle_name: &str, last_name: &str, age: u8) -> Self {
        let mut rng = rand::rngs::StdRng::from_entropy();
        Person {
            id: Vec::from(rng.gen::<[u8; 32]>()),
            owner_id: Vec::from(rng.gen::<[u8; 32]>()),
            first_name: first_name.to_string(),
            middle_name: middle_name.to_string(),
            last_name: last_name.to_string(),
            age,
        }
    }

    fn random_people(count: u32, seed: Option<u64>) -> Vec<Self> {
        let first_names =
            common::text_file_strings("src/supporting_files/contract/family/first-names.txt");
        let middle_names =
            common::text_file_strings("src/supporting_files/contract/family/middle-names.txt");
        let last_names =
            common::text_file_strings("src/supporting_files/contract/family/last-names.txt");
        let mut vec: Vec<Person> = vec![];

        let mut rng = match seed {
            None => rand::rngs::StdRng::from_entropy(),
            Some(seed_value) => rand::rngs::StdRng::seed_from_u64(seed_value),
        };

        for _i in 0..count {
            let person = Person {
                id: Vec::from(rng.gen::<[u8; 32]>()),
                owner_id: Vec::from(rng.gen::<[u8; 32]>()),
                first_name: first_names.choose(&mut rng).unwrap().clone(),
                middle_name: middle_names.choose(&mut rng).unwrap().clone(),
                last_name: last_names.choose(&mut rng).unwrap().clone(),
                age: rng.gen_range(0..85),
            };
            vec.push(person);
        }
        vec
    }

    fn from_document(document: Document) -> Person {
        let first_name = document
            .properties
            .get("firstName")
            .expect("we should be able to get the first name")
            .as_text()
            .expect("the first name should be a string")
            .to_string();
        let middle_name = document
            .properties
            .get("middleName")
            .expect("we should be able to get the middle name")
            .as_text()
            .expect("the middle name should be a string")
            .to_string();
        let last_name = document
            .properties
            .get("lastName")
            .expect("we should be able to get the last name")
            .as_text()
            .expect("the last name should be a string")
            .to_string();
        let age: u8 = document
            .properties
            .get("age")
            .expect("we should be able to get the age")
            .as_integer()
            .expect("the age should be an integer")
            .try_into()
            .expect("expected u8 value");

        Person {
            id: document.id.to_vec(),
            owner_id: document.owner_id.to_vec(),
            first_name,
            middle_name,
            last_name,
            age,
        }
    }

    fn add_single(&self, drive: &Drive, contract: &Contract) -> (i64, u64) {
        let db_transaction = drive.grove.start_transaction();
        let result = self.add_on_transaction(drive, contract, &db_transaction);
        drive
            .grove
            .commit_transaction(db_transaction)
            .map_err(|err| {
                println!("### ERROR! Unable to commit transaction");
                println!("### Info {:?}", err);
            })
            .unwrap()
            .expect("expected to commit transaction");
        result
    }

    fn add_on_transaction(
        &self,
        drive: &Drive,
        contract: &Contract,
        db_transaction: &Transaction,
    ) -> (i64, u64) {
        let storage_flags = StorageFlags { epoch: 0 };
        let value = serde_json::to_value(&self).expect("serialized person");
        let document_cbor =
            common::value_to_cbor(value, Some(rs_drive::drive::defaults::PROTOCOL_VERSION));
        let document = Document::from_cbor(document_cbor.as_slice(), None, None)
            .expect("document should be properly deserialized");

        let document_type = contract
            .document_type_for_name("person")
            .expect("expected to get person contract");
        drive
            .add_document_for_contract(
                DocumentAndContractInfo {
                    document_info: DocumentAndSerialization((
                        &document,
                        &document_cbor,
                        &storage_flags,
                    )),
                    contract,
                    document_type,
                    owner_id: None,
                },
                true,
                0f64,
                true,
                Some(db_transaction),
            )
            .expect("document should be inserted")
    }

    fn println(&self) {
        println!(
            "{} {} {} {} {}",
            bs58::encode(&self.id).into_string(),
            self.first_name,
            self.middle_name,
            self.last_name,
            self.age
        )
    }
}

pub fn populate(count: u32, drive: &Drive, contract: &Contract) -> Result<(), Error> {
    let db_transaction = drive.grove.start_transaction();

    let people = Person::random_people(count, None);
    for person in people {
        person.add_on_transaction(drive, contract, &db_transaction);
    }
    drive.commit_transaction(db_transaction)?;

    Ok(())
}

fn prompt(name: &str) -> String {
    let mut line = String::new();
    print!("{}", name);
    std::io::stdout().flush().unwrap();
    std::io::stdin()
        .read_line(&mut line)
        .expect("Error: Could not read a line");

    return line.trim().to_string();
}

fn print_person_contract_options() {
    println!();
    println!("##############################################################");
    println!("### You have the following options in the person contract: ###");
    println!("##############################################################");
    println!();
    println!(
        "### pop <number>                                       - populate with number people"
    );
    println!("### insert <firstName> <middleName> <lastName> <age>   - add a specific person");
    println!("### delete <id>                                        - remove a person by id");
    println!("### all <[sortBy1,sortBy2...]> <limit>                 - get all people sorted by defined fields");
    println!(
        "### query <sqlQuery>                                   - sql like query on the system"
    );
    println!(
        "### cost <document_type_name>                         - get the worst case scenario insertion cost"
    );
    println!();
}

fn prompt_populate(input: String, drive: &Drive, contract: &Contract) {
    let args: Vec<&str> = input.split_whitespace().collect();
    if args.len() != 2 {
        println!("### ERROR! Only one parameter should be provided");
    } else if let Some(count_str) = args.last() {
        match count_str.parse::<u32>() {
            Ok(value) => {
                if value > 0 && value <= 5000 {
                    let start_time = SystemTime::now();
                    populate(value, drive, contract).expect("populate returned an error");
                    if let Ok(n) = SystemTime::now().duration_since(start_time) {
                        println!("Time taken: {}", n.as_secs_f64());
                    }
                } else {
                    println!("### ERROR! Value must be between 1 and 1000");
                }
            }
            Err(_) => {
                println!("### ERROR! An integer was not provided");
            }
        }
    }
}

fn prompt_insert(input: String, drive: &Drive, contract: &Contract) {
    let args = input.split_whitespace();
    if args.count() != 5 {
        println!("### ERROR! Four parameter should be provided");
    } else {
        let split: Vec<String> = input.split_whitespace().map(|s| s.to_string()).collect();
        let first_name = split.get(1).unwrap();
        let middle_name = split.get(2).unwrap();
        let last_name = split.get(3).unwrap();
        let age_string = split.get(4).unwrap();
        match age_string.parse::<u8>() {
            Ok(age) => {
                if age <= 150 {
                    let start_time = SystemTime::now();
                    let (storage_fee, processing_fee) =
                        Person::new_with_random_ids(first_name, middle_name, last_name, age)
                            .add_single(drive, contract);
                    if let Ok(n) = SystemTime::now().duration_since(start_time) {
                        println!(
                            "Storage fee: {} ({})",
                            storage_fee,
                            (storage_fee as f64) * 10_f64.pow(-11) * DASH_PRICE
                        );
                        println!(
                            "Processing fee: {} ({})",
                            processing_fee,
                            (processing_fee as f64) * 10_f64.pow(-11) * DASH_PRICE
                        );
                        println!("Time taken: {}", n.as_secs_f64());
                    }
                } else {
                    println!("### ERROR! Age must be under 150");
                }
            }
            Err(_) => {
                println!("### ERROR! An integer was not provided");
            }
        }
    }
}

fn prompt_delete(input: String, drive: &Drive, contract: &Contract) {
    let args = input.split_whitespace();
    if args.count() != 2 {
        println!("### ERROR! Two parameter should be provided");
    } else {
        let split: Vec<String> = input.split_whitespace().map(|s| s.to_string()).collect();
        let id_bs58 = split.get(1).unwrap().as_str();
        let id = bs58::decode(id_bs58).into_vec();
        if id.is_err() {
            println!("### ERROR! Could not decode id");
        }
        let id = id.unwrap();
        if drive
            .delete_document_for_contract(id.as_slice(), contract, "person", None, true, None)
            .is_err()
        {
            println!("### ERROR! Could not delete document");
        }
    }
}

fn prompt_query(input: String, drive: &Drive, contract: &Contract) {
    let query = DriveQuery::from_sql_expr(input.as_str(), &contract).expect("should build query");
    let results = query.execute_no_proof(&drive, None);
    if let Ok((results, _, processing_fee)) = results {
        let people: Vec<Person> = results
            .into_iter()
            .map(|result| {
                let document = Document::from_cbor(result.as_slice(), None, None)
                    .expect("we should be able to deserialize the cbor");
                Person::from_document(document)
            })
            .collect();
        println!("processing fee is {}", processing_fee);
        people.iter().for_each(|person| person.println());
    } else {
        println!("invalid query, try again");
    }
}

fn prompt_cost(input: String, drive: &Drive, contract: &Contract) {
    let args = input.split_whitespace();
    if args.count() != 2 {
        println!("### ERROR! Two parameter should be provided");
    } else {
        let doument_type_name = input.split_whitespace().last().unwrap();
        let document_type_result = contract.document_type_for_name(doument_type_name);
        match document_type_result {
            Ok(_) => {
                match drive.worst_case_fee_for_document_type_with_name(contract, doument_type_name)
                {
                    Ok((storage_fee, processing_fee)) => {
                        println!(
                            "The storage fee is {}, processing fee is {}",
                            storage_fee, processing_fee
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

fn all(order_by_strings: Vec<String>, limit: u16, drive: &Drive, contract: &Contract) {
    println!("{:?} {:?}", order_by_strings, limit);
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
    let person_document_type = contract
        .document_types()
        .get("person")
        .expect("contract should have a person document type");
    let query = DriveQuery {
        contract,
        document_type: person_document_type,
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
    let people: Vec<Person> = results
        .into_iter()
        .map(|result| {
            let document = Document::from_cbor(result.as_slice(), None, None)
                .expect("we should be able to deserialize the cbor");
            Person::from_document(document)
        })
        .collect();
    println!("processing fee is {}", processing_fee);
    people.iter().for_each(|person| person.println());
}

fn prompt_all(input: String, drive: &Drive, contract: &Contract) {
    let args = input.split_whitespace();
    if args.count() > 3 {
        println!("### ERROR! At max two parameters should be provided");
    } else {
        let split: Vec<String> = input.split_whitespace().map(|s| s.to_string()).collect();
        let arg0 = split.get(1);
        let arg1 = split.get(2);
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
        if order_by.is_empty() {
            order_by = vec!["firstName".to_string()];
        }
        all(order_by, limit, drive, contract);
    }
}

fn person_rl(drive: &Drive, contract: &Contract, rl: &mut Editor<()>) -> bool {
    let readline = rl.readline("> ");
    match readline {
        Ok(input) => {
            if input.starts_with("pop ") {
                prompt_populate(input, &drive, &contract);
                true
            } else if input.starts_with("all") {
                prompt_all(input, &drive, &contract);
                true
            } else if input.starts_with("insert ") {
                prompt_insert(input, &drive, &contract);
                true
            } else if input.starts_with("delete ") {
                prompt_delete(input, &drive, &contract);
                true
            } else if input.starts_with("select ") {
                prompt_query(input, &drive, &contract);
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

pub fn person_loop(drive: &Drive, contract: &Contract, rl: &mut Editor<()>) -> bool {
    print_person_contract_options();
    person_rl(drive, contract, rl)
}
