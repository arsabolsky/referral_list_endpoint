// Jackson Coxson & Karter Arritt

use chrono::{ Duration, Utc };
use church::ChurchClient;
use indicatif::ProgressBar;
use std::time::Duration as dur;

mod bearer;
mod church;
mod env;
mod persons;
mod send;

const CLI_OPTIONS: [&str; 2] = ["send timeline", "exit"];
const CLI_DESCRIPTONS: [&str; 2] = [
    "Gets all the timeline events and send a person by person score to a Web Endpoint",
    "Exits the program",
];

#[tokio::main]
async fn main() {
    let spinner = ProgressBar::new_spinner();
    spinner.enable_steady_tick(dur::from_millis(100));
    println!("Starting referral list endpoint program... Please wait...");
    let save_env = env::check_vars();
    env_logger::init();
    let mut church_client: ChurchClient = church::ChurchClient::new(save_env).await.unwrap();
    spinner.finish();


    let mut args = std::env::args().skip(1); // Skip the first argument (binary name)
    if let Some(arg) = args.next() {
        if let Err(e) = parse_argument(&arg, &mut church_client).await {
            println!("Ran into an error while processing: {e:?}");
        }
        return;
    }
    let _ = send(&mut church_client).await;
}

async fn parse_argument(arg: &str, church_client: &mut ChurchClient) -> anyhow::Result<bool> {
    match arg {
        "send timeline" => send(church_client).await,
        "exit" => Ok(false),
        "help" | "-h" => {
            println!(
                "Referral List - a tool to get and parse a list of referrals from referral manager."
            );
            for i in 0..CLI_OPTIONS.len() {
                println!("  {} - {}", CLI_OPTIONS[i], CLI_DESCRIPTONS[i]);
            }
            Ok(false)
        }
        _ => Err(anyhow::anyhow!("Unknown usage '{arg}' - run without arguments to see options")),
    }
}

async fn send(church_client: &mut ChurchClient) -> anyhow::Result<bool>{
    let da_peeps = store_timeline(church_client).await?;

    let out = persons::convert_referral_to_gas(da_peeps);

    let encrypted_data = match
        send::encrypt_struct_with_otp(
            out,
            church_client.env.timeline_send_crypt_key.clone()
        )
    {
        Ok(data) => data,
        Err(e) => {
            println!("Error encrypting data: {}", e);
            return Ok(false); // or return Err(e) if needed
        }
    };

    match
        send::send_to_google_apps_script(
            encrypted_data,
            church_client.env.timeline_send_url.clone()
        ).await
    {
        Ok(_) => {
            //println!("Success! Decrypted response: {}", decrypted_json);
        }
        Err(e) => {
            eprintln!("Error sending request: {}", e);
        }
    }

    Ok(true)
}

pub async fn store_timeline(
    church_client: &mut ChurchClient
) -> anyhow::Result<Vec<persons::ReferralPerson>> {
    let persons_list = church_client.get_cached_people_list().await?.to_vec();
    let now = Utc::now().naive_utc();
    let persons_list: Vec<persons::Person> = persons_list
        .into_iter()
        .filter(|x| {
            x.person_status < persons::PersonStatus::NewMember &&
                now.signed_duration_since(x.assigned_date) < Duration::days(8)
        })
        .collect();

    let mut da_peeps = Vec::new();
    let bar = ProgressBar::new(persons_list.len() as u64);
    for person in persons_list {
        bar.inc(1);
        let t: Vec<persons::TimelineEvent> = if
            let Ok(t) = church_client.get_person_timeline(&person).await
        {
            t.iter()
                .filter(
                    |event|
                        matches!(
                            event.item_type,
                            persons::TimelineItemType::Contact |
                                persons::TimelineItemType::Teaching |
                                persons::TimelineItemType::NewReferral
                        ) &&
                        (if
                            event.item_type != persons::TimelineItemType::NewReferral &&
                            event.status.is_none()
                        {
                            false
                        } else {
                            true
                        })
                )
                .cloned()
                .collect()
        } else {
            continue;
        };
        let cont_time: usize;
        if let Some(t) = church_client.get_person_contact_time(&person).await? {
            cont_time = t;
            //t;
        } else {
            continue;
        }
        let mut this_guy = persons::ReferralPerson::new(
            person.guid,
            person.first_name,
            cont_time,
            t.clone(),
            match person.area_name {
                Some(s) => s.clone(), // return the String if present
                None => String::from("default_area"), // return a default value if None
            },
            match person.referral_status {
                persons::ReferralStatus::NotAttempted => "Not Attempted",
                persons::ReferralStatus::NotSuccessful => "Unsuccessful",
                persons::ReferralStatus::Successful => "Successful",
            }.to_string()
        );

        let yesterday = chrono::Local::now().naive_utc().date() - Duration::days(1);

        let last_new_referral = t
            .iter()
            .find(|event| { event.item_type == persons::TimelineItemType::NewReferral });

        let mut current_date: chrono::NaiveDate = last_new_referral.unwrap().item_date.date();
        let mut contact_days = 0;
        let mut total_days = 0;

        while current_date <= yesterday && total_days < 7 {
            total_days += 1;

            let c = check_day(current_date, t.clone());
            if c == -1 {
                contact_days += 1;
                break;
            } else {
                contact_days += c;
            }

            current_date = current_date + Duration::days(1);
        }

        this_guy.set_score(format!("{contact_days}/{total_days}"));

        da_peeps.push(this_guy);
    }
    bar.finish();

    church_client.env.save_data(&da_peeps)?;

    Ok(da_peeps)
}

fn check_day(day: chrono::naive::NaiveDate, person: Vec<persons::TimelineEvent>) -> i32 {
    // Find all events that match the day and the 'Contact' type
    let events_on_day: Vec<&persons::TimelineEvent> = person
        .iter()
        .filter(
            |event|
                event.item_date.date() == day &&
                (event.item_type == persons::TimelineItemType::Contact ||
                    event.item_type == persons::TimelineItemType::Teaching)
        )
        .collect();

    // If there are no events for the day, return 0
    if events_on_day.is_empty() {
        return 0;
    }

    // Check each event. If any event has a status of Some(true), return -1
    for event in events_on_day {
        if event.status.unwrap_or(false) {
            return -1;
        }
    }

    // If no events with status Some(true) were found, return 1
    1
}