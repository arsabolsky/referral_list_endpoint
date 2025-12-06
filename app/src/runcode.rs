use crate::env::Env;
use base64;
use base64::{engine::general_purpose, Engine};
use dialoguer::{theme::ColorfulTheme, Input, Select};
use serde_json; // Assuming the Env struct is in `env.rs`

pub fn check_for_runcode() -> Option<Env> {
    let args: Vec<String> = std::env::args().collect();

    let base64_string: String = if args.len() >= 2 {
        println!("Running with Runcode: {}", args[1]);
        args[1].clone()
    } else {
        Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Runcode? If unsure, <enter>")
            .default("/".to_string()) // Default to "/" if the user doesn't enter anything
            .interact()
            .unwrap()
    };

    // If the user just presses Enter and defaults to "/", return None
    if base64_string == "/" {
        return None;
    }

    // Decode the base64 string
    match general_purpose::STANDARD.decode(&base64_string) {
        Ok(decoded) => {
            // Convert the decoded bytes into a UTF-8 string
            if let Ok(decoded_str) = String::from_utf8(decoded) {
                // Try to parse the JSON string into an Env struct
                match build_env_from_runcode(&decoded_str) {
                    Ok(env) => {
                        return Some(env);
                    }
                    Err(e) => {
                        println!("Error parsing JSON into Env: {}", e);
                        return None;
                    }
                }
            } else {
                println!("Decoded data is not valid UTF-8");
                return None;
            }
        }
        Err(e) => {
            println!("Error decoding base64: {}", e);
            return None;
        }
    }
}

pub fn build_env_from_runcode(decoded_str: &str) -> Result<Env, serde_json::Error> {
    // Parse the decoded JSON string into an Env struct
    let env: crate::env::Env = serde_json::from_str(decoded_str)?;
    Ok(env)
}

pub fn build_base64_runcode_from_env(env: &Env) -> Option<String> {
    let selections = &["Yes", "No"];
    match Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Do you want your configuration as a runcode?")
        .default(1)
        .items(selections)
        .interact()
        .unwrap()
    {
        0usize => {
            Some(match serde_json::to_string(env) {
                Ok(json_string) => {
                    // Step 2: Encode the JSON string into Base64
                    let base64_string = general_purpose::STANDARD.encode(json_string);
                    base64_string
                }
                Err(e) => {
                    println!("Error serializing Env struct: {}", e);
                    String::new() // Return an empty string if there was an error
                }
            })
        }
        1usize => None,
        _ => None,
    }

    // Step 1: Serialize the Env struct into a JSON string
}
