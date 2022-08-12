use {
    flatbuffers::{ForwardsUOffset, Vector},
    lazy_static::lazy_static,
    regex::{Captures, Regex},
    solana_sdk::pubkey::Pubkey,
};

pub fn filter_events_from_logs(log_messages: &Vec<&str>) -> Result<Vec<String>, ()> {
    lazy_static! {
        static ref CLRE: Regex = Regex::new(
            r"Program data: ((?:[A-Za-z\d+/]{4})*(?:[A-Za-z\d+/]{3}=|[A-Za-z\d+/]{2}==)?$)"
        )
        .unwrap();
    }
    let mut events: Vec<String> = vec![];

    for line in log_messages {
        let line_str = String::from(*line);
        let captures = CLRE.captures(&line_str);
        let b64raw = captures.and_then(|c| c.get(1)).map(|c| c.as_str());
        b64raw.map(|raw| events.push((raw).parse().unwrap()));
    }
    if events.is_empty() {
        println!("No events captured!");
        Err(())
    } else {
        Ok(events)
    }
}

pub fn parse_logs<'a>(
    log_messages: Option<Vector<'a, ForwardsUOffset<&str>>>,
) -> Result<Vec<(Pubkey, Vec<&'a str>, u8)>, ()> {
    lazy_static! {
        static ref PLRE: Regex = Regex::new(r"Program (\w*) invoke \[(\d)\]").unwrap();
    }
    let mut program_logs: Vec<(Pubkey, Vec<&str>, u8)> = vec![];

    match log_messages {
        Some(logs) => {
            for log in logs {
                let captures: Option<Captures> = PLRE.captures(log);
                let cap: Option<(Pubkey, u8)> = captures.and_then(|c| {
                    let program = c
                        .get(1)
                        .and_then(|prog| bs58::decode(&prog.as_str()).into_vec().ok())
                        .map(|bytes| Pubkey::new(&bytes));
                    let level: Option<u8> = c.get(2).and_then(|l| l.as_str().parse::<u8>().ok());
                    if program.is_some() && level.is_some() {
                        return Some((program.unwrap(), level.unwrap()));
                    }
                    None
                });

                match cap {
                    Some((key, level)) if level == 1 => {
                        program_logs.push((key, vec![], level));
                    }
                    _ => {
                        let last_program_log = program_logs.last_mut().unwrap();
                        (*last_program_log).1.push(log);
                    }
                }
            }
            Ok(program_logs)
        }
        None => {
            println!("No logs found in transaction info!");
            Err(())
        }
    }
}
