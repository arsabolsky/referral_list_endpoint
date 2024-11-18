//Jackson Coxson & Karter Arritt

use chrono::{Duration, Utc};
use church::ChurchClient;
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::sync::Arc;
use std::time::Duration as Dur;
use tokio::sync::{Mutex, Semaphore};
mod bearer;
mod church;
mod env;
mod persons;
mod send;

#[tokio::main]
async fn main() {

    // Wrap MultiProgress in a Mutex so it can be safely shared and accessed
    let m = Arc::new(Mutex::new(MultiProgress::new()));

    println!("Running referral_list_enpoint, please wait...");
    let env_set_bar = {
        let m = m.lock().await;
        m.add(ProgressBar::new(1)) // Only 1 step to indicate loading
    };
    env_set_bar.set_style(ProgressStyle::default_bar()
        .template("{spinner} {msg}").unwrap());
    env_set_bar.set_message("Loading .env data...");
    let save_env = env::check_vars();
    env_logger::init();
    env_set_bar.inc(1);
    env_set_bar.finish_with_message(".env load finished!");

    let church_client_bar = {
        let m = m.lock().await;
        m.add(ProgressBar::new(1))
    };
    church_client_bar.set_style(ProgressStyle::default_bar()
        .template("{spinner} {msg}").unwrap());
    church_client_bar.set_message("Loading Church Client data...");
    // Wrap ChurchClient in tokio's Mutex
    let church_client = Arc::new(Mutex::new(church::ChurchClient::new(save_env).await.unwrap()));
    church_client_bar.inc(1);
    church_client_bar.finish_with_message("Church Client load finished!");

    // Pass the wrapped ChurchClient to store_timeline
    let _ = send(Arc::clone(&m), church_client).await;  // Pass the cloned Arc
}

async fn send(m: Arc<Mutex<MultiProgress>>, church_client: Arc<Mutex<ChurchClient>>) -> anyhow::Result<bool> {

    let da_peeps = store_timeline(Arc::clone(&m), Arc::clone(&church_client)).await?;

    let send_bar = {
        let m = m.lock().await;
        m.add(ProgressBar::new(3))
    };
    send_bar.set_style(ProgressStyle::default_bar()
        .template("{spinner} {msg}").unwrap());
    send_bar.set_message("Encrypting and Sending data...");

    let out = persons::convert_referral_to_gas(da_peeps);
    send_bar.inc(1);

    let encrypted_data = {
        let church_client = church_client.lock().await;
        match send::encrypt_struct_with_otp(out, church_client.env.timeline_send_crypt_key.clone()) {
            Ok(data) => data,
            Err(e) => {
                println!("Error encrypting data: {}", e);
                return Ok(false); // or return Err(e) if needed
            }
        }
    };
    send_bar.inc(1);

    match send::send_to_google_apps_script(
        encrypted_data,
        {
            let church_client = church_client.lock().await;
            church_client.env.timeline_send_url.clone()
        },
    )
    .await
    {
        Ok(_) => {
            //println!("Success! Decrypted response: {}", decrypted_json);
        }
        Err(e) => {
            eprintln!("Error sending request: {}", e);
        }
    }
    send_bar.inc(1);
    send_bar.finish_with_message("Data Encrypted and Sent!");

    Ok(true)
}

pub async fn store_timeline(
    m: Arc<Mutex<MultiProgress>>,
    church_client: Arc<Mutex<ChurchClient>> // Now using tokio::sync::Mutex
) -> anyhow::Result<Vec<persons::ReferralPerson>> {
    let persons_list = {
        let mut church_client = church_client.lock().await;  // Lock using tokio::sync::Mutex
        church_client.get_cached_people_list().await?.to_vec()
    };

    let now = Utc::now().naive_utc();
    let persons_list: Vec<persons::Person> = persons_list
        .into_iter()
        .filter(|x| {
            x.person_status < persons::PersonStatus::NewMember &&
                now.signed_duration_since(x.assigned_date) < Duration::days(8)
        })
        .collect();

    let person_overall_bar = {
        let m = m.lock().await;
        m.add(ProgressBar::new(persons_list.len() as u64))
    };
    person_overall_bar.set_style(ProgressStyle::default_bar()
        .template("{wide_bar} ({percent}%) {eta:4} {msg}")?);
    person_overall_bar.set_message("Retrieving/Processing person records...");

    // Create a semaphore with a limit of 10 concurrent tasks
    let semaphore = Arc::new(Semaphore::new(10));

    let mut tasks = Vec::new(); // to store task handles

    for person in persons_list {
        // Clone the Arc for each task
        let m = Arc::clone(&m);
        let church_client = Arc::clone(&church_client);
        let semaphore = Arc::clone(&semaphore);  // Clone the semaphore as well
        let task = tokio::spawn(async move {
            // Try to acquire a permit from the semaphore
            let _permit = semaphore.acquire().await.unwrap();  // This will block if there are 10 tasks running

            let person_bar = {
                let m = m.lock().await;
                m.add(ProgressBar::new(3))
            };
            person_bar.set_style(ProgressStyle::default_bar()
                .template("{spinner} {msg}").unwrap());
            person_bar.set_message(format!("Processing person: {}", person.first_name));
            person_bar.enable_steady_tick(Dur::from_millis(100));

            let t: Vec<persons::TimelineEvent> = {
                let mut church_client = church_client.lock().await;  // Lock using tokio::sync::Mutex
                if let Ok(t) = church_client.get_person_timeline(&person).await {
                    t.iter()
                        .filter(
                            |event| matches!(
                                event.item_type,
                                persons::TimelineItemType::Contact |
                                persons::TimelineItemType::Teaching |
                                persons::TimelineItemType::NewReferral
                            ) &&
                            (if event.item_type != persons::TimelineItemType::NewReferral && event.status.is_none() {
                                false
                            } else {
                                true
                            })
                        )
                        .cloned()
                        .collect()
                } else {
                    return None;
                }
            };

            let cont_time = {
                let mut church_client = church_client.lock().await;  // Lock using tokio::sync::Mutex
                match church_client.get_person_contact_time(&person).await {
                    Ok(Some(t)) => t,
                    _ => return None,
                }
            };

            let mut this_guy = persons::ReferralPerson::new(
                person.guid,
                person.first_name,
                cont_time,
                t.clone(),
                match person.area_name {
                    Some(s) => s.clone(),
                    None => String::from("default_area"),
                },
                match person.referral_status {
                    persons::ReferralStatus::NotAttempted => "Not Attempted",
                    persons::ReferralStatus::NotSuccessful => "Unsuccessful",
                    persons::ReferralStatus::Successful => "Successful",
                }.to_string()
            );

            let yesterday = chrono::Local::now().naive_utc().date() - Duration::days(1);
            let last_new_referral = t.iter().find(|event| event.item_type == persons::TimelineItemType::NewReferral);
            let mut current_date = last_new_referral.unwrap().item_date.date();
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

            person_bar.finish_and_clear();
            Some(this_guy)
        });

        tasks.push(task);
    }

    // Wait for all tasks to finish and collect their results
    let mut da_peeps = Vec::new();
    for task in tasks {
        match task.await.unwrap() {
            Some(person) => {
                da_peeps.push(person);
            }
            None => {
                // Handle case where the task didn't return a valid person
            }
        }
        person_overall_bar.inc(1);
    }

    person_overall_bar.finish_with_message("Person Records Processed!");

    let church_client = church_client.lock().await;  // Lock before saving
    church_client.env.save_data(&da_peeps)?;

    Ok(da_peeps)
}

fn check_day(day: chrono::naive::NaiveDate, person: Vec<persons::TimelineEvent>) -> i32 {
    // Find all events that match the day and the 'Contact' type
    let events_on_day: Vec<&persons::TimelineEvent> = person
        .iter()
        .filter(|event| {
            event.item_date.date() == day
                && (event.item_type == persons::TimelineItemType::Contact
                    || event.item_type == persons::TimelineItemType::Teaching)
        })
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
