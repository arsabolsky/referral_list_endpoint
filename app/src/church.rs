// Jackson Coxson
// Code to interact with church servers

use anyhow::Context;
use chrono::NaiveDateTime;
use log::info;
use reqwest::{redirect::Policy, Client};
use reqwest_cookie_store::CookieStoreMutex;
use serde::Deserialize;
use serde_json::json;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{bearer::BearerToken, env, persons};

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/93.0.4577.82 Safari/537.36";
const MAX_RETRIES: u8 = 3;

// These constants are left for future use and clarity of the OAuth2 flow
#[allow(dead_code)]
const AUTH_BASE_URL: &str = "https://id.churchofjesuschrist.org";
#[allow(dead_code)]
const REFERRAL_BASE_URL: &str = "https://referralmanager.churchofjesuschrist.org";
#[allow(dead_code)]
const OAUTH_CLIENT_ID: &str = "0oaodd1guy51rqnJo357";
#[allow(dead_code)]
const CACHE_TTL_SECS: u64 = 60 * 60; // 1 hour

// These structures are used during deserialization in the login flow
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct InteractResponse {
    #[serde(rename = "interaction_handle")]
    interaction_handle: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct StateHandleResponse {
    #[serde(rename = "stateHandle")]
    state_handle: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AuthServiceResponse {
    token: String,
}

#[derive(Debug)]
pub struct ChurchClient {
    http_client: Client,
    cookie_store: Arc<CookieStoreMutex>,
    pub env: env::Env,
    bearer_token: Option<BearerToken>,
}

impl ChurchClient {
    pub async fn new(env: env::Env) -> anyhow::Result<Self> {
        let working_path = PathBuf::from(&env.working_path);
        let bearer_path = working_path.join("bearer.token");
        let cookies_path = working_path.join("cookies.json");

        // Load existing bearer token if available
        let bearer_token = std::fs::read_to_string(&bearer_path)
            .ok()
            .and_then(|b| BearerToken::from_base64(b).ok())
            .inspect(|_| info!("Loaded cached bearer token"));

        // Ensure cookies file exists
        if !cookies_path.exists() {
            std::fs::write(&cookies_path, b"")?;
        }

        let cookie_store = Self::load_cookie_store(&cookies_path)?;
        let http_client = Self::build_http_client(&cookie_store)?;

        Ok(Self {
            http_client,
            cookie_store,
            env,
            bearer_token,
        })
    }

    fn load_cookie_store(
        cookies_path: &PathBuf,
    ) -> anyhow::Result<Arc<CookieStoreMutex>> {
        let cookie_store = if cookies_path.exists() {
            let file = std::fs::File::open(cookies_path)
                .map(std::io::BufReader::new)
                .ok();
            if let Some(file) = file {
                #[allow(deprecated)]
                reqwest_cookie_store::CookieStore::load_json(file).unwrap_or_else(|_| {
                    #[allow(deprecated)]
                    let store = reqwest_cookie_store::CookieStore::default();
                    store
                })
            } else {
                #[allow(deprecated)]
                let store = reqwest_cookie_store::CookieStore::default();
                store
            }
        } else {
            #[allow(deprecated)]
            let store = reqwest_cookie_store::CookieStore::default();
            store
        };

        Ok(Arc::new(CookieStoreMutex::new(cookie_store)))
    }

    fn build_http_client(
        cookie_store: &Arc<CookieStoreMutex>,
    ) -> anyhow::Result<Client> {
        Client::builder()
            .user_agent(USER_AGENT)
            .cookie_provider(Arc::clone(cookie_store))
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() > 2 {
                    info!("Stopping redirect chain at {} redirects", attempt.previous().len());
                    attempt.stop()
                } else {
                    info!("Following redirect to {}", attempt.url());
                    attempt.follow()
                }
            }))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .context("Failed to build HTTP client")
    }

    pub async fn save_cookies(&self) -> anyhow::Result<()> {
        info!("Saving cookies");
        let cookies_path = PathBuf::from(&self.env.working_path).join("cookies.json");
        let mut file = std::fs::File::create(&cookies_path)
            .map(std::io::BufWriter::new)?;

        let store = self.cookie_store.lock().unwrap();
        #[allow(deprecated)]
        store
            .save_incl_expired_and_nonpersistent_json(&mut file)
            .map_err(|e| anyhow::anyhow!("Failed to save cookies: {}", e))?;

        Ok(())
    }

    async fn write_bearer_token(&self, token: &str) -> anyhow::Result<()> {
        info!("Saving bearer token");
        let bearer_path = PathBuf::from(&self.env.working_path).join("bearer.token");
        std::fs::write(&bearer_path, token).context("Failed to write bearer token")?;
        Ok(())
    }

    /// Logs into churchofjesuschrist.org
    pub async fn login(&mut self) -> anyhow::Result<BearerToken> {
        info!("Logging into referral manager");
        self.cookie_store.lock().unwrap().clear();
        use serde::Deserialize;

        use sha2::{Digest, Sha256};
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        use rand::{distributions::Alphanumeric, Rng};

        #[derive(Deserialize)]
        struct InteractResponse {
            #[serde(rename = "interaction_handle")]
            interaction_handle: String,
        }

        #[derive(Deserialize)]
        struct StateHandle {
            #[serde(rename = "stateHandle")]
            state_handle: String,
        }

        fn generate_random_string(len: usize) -> String {
            rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(len)
                .map(char::from)
                .collect()
        }

        fn generate_code_challenge(verifier: &str) -> String {
            let hash = Sha256::digest(verifier.as_bytes());
            URL_SAFE_NO_PAD.encode(hash)
        }

        info!("Generating PKCE and security parameters");
        let code_verifier = generate_random_string(64);
        let code_challenge = generate_code_challenge(&code_verifier);
        let state = generate_random_string(32);
        let nonce = generate_random_string(32);
        // println!("code_verifier: {}", code_verifier);
        // println!("code_challenge: {}", code_challenge);
        // println!("state: {}", state);
        // println!("nonce: {}", nonce);
        
        // First, visit referralmanager.churchofjesuschrist.org to capture the redirect Location header
        info!("Visiting referralmanager.churchofjesuschrist.org to capture redirect");
        let no_redirect_client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .cookie_provider(Arc::clone(&self.cookie_store))
            .redirect(Policy::none())
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        let redirect_res = no_redirect_client
            .get("https://referralmanager.churchofjesuschrist.org/")
            .send()
            .await?;

        // println!("referralmanager redirect status: {}", redirect_res.status());
        let mut location_url = String::new();
        if let Some(loc) = redirect_res.headers().get(reqwest::header::LOCATION) {
            let location = loc.to_str().unwrap_or("<invalid-utf8>");
            // println!("referralmanager Location header: {}", location);
            location_url = location.to_string();
        }
        for (_name, _value) in redirect_res.headers().iter() {
            // Headers intentionally unused - preserved for future debugging
        }

        // Follow the Location URL
        if !location_url.is_empty() {
            info!("Following redirect to: {}", location_url);
            let location_res = self
                .http_client
                .get(&location_url)
                .send()
                .await?;

            // println!("Location URL response status: {}", location_res.status());
            let _location_body = location_res.text().await?;
            // println!("Location URL response body (len={}): {}", location_body.len(), location_body);
        }

        info!("Calling /interact to get interactionHandle");
        let interact_response = self
            .http_client
            .post("https://id.churchofjesuschrist.org/oauth2/default/v1/interact")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", "0oaodd1guy51rqnJo357"),
                ("scope", "openid profile offline_access"),
                ("redirect_uri", "https://referralmanager.churchofjesuschrist.org/login"),
                ("code_challenge", &code_challenge),
                ("code_challenge_method", "S256"),
                ("state", &state),
                ("nonce", &nonce),
            ])
            .send()
            .await?
            .json::<InteractResponse>()
            .await?;

        let interaction_handle = interact_response.interaction_handle;
        // println!("interaction_handle: {}", interaction_handle);

        info!("Calling /introspect with interactionHandle");
        let state_handle = self
            .http_client
            .post("https://id.churchofjesuschrist.org/idp/idx/introspect")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(json!({ "interactionHandle": interaction_handle }).to_string())
            .send()
            .await?
            .json::<StateHandle>()
            .await?
            .state_handle;

        // Send the username
        info!("Sending the username");
        let body = json!({
            "stateHandle": state_handle,
            "identifier": self.env.church_username
        })
        .to_string();

        // Send the username to get the state handle
        let identify_response: serde_json::Value = self
            .http_client
            .post("https://id.churchofjesuschrist.org/idp/idx/identify")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(body)
            .send()
            .await?
            .json()
            .await?;

        // Extract the state handle from the response
        let state_handle = identify_response["stateHandle"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No state handle in identify response"))?
            .to_string();

        // Find password authenticator id
        // Authenticators: Array [Object {"allowedFor": String("any"), "displayName": String("Email"), "id": String("---"), "key": String("okta_email"), "methods": Array [Object {"type": String("email")}], "type": String("email")}, Object {"allowedFor": String("sso"), "displayName": String("Password"), "id": String("---"), "key": String("okta_password"), "methods": Array [Object {"type": String("password")}], "type": String("password")}]
        let password_authenticator_id = identify_response["authenticators"]["value"]
            .as_array()
            .and_then(|authenticators| {
                authenticators.iter().find_map(|auth| {
                    if auth["type"] == "password" {
                        Some(auth["id"].as_str()?.to_string())
                    } else {
                        None
                    }
                })
            })
            .ok_or_else(|| anyhow::anyhow!("No password authenticator found"))?;

            // println!("password_authenticator_id: {}", password_authenticator_id);

        // Challenge the password authenticator
        info!("Challenging the password authenticator");
        let body = json!({
            "authenticator": {
                "id": password_authenticator_id
            },
            "stateHandle": state_handle
        })
        .to_string();
        let challenge_response: serde_json::Value = self
            .http_client
            .post("https://id.churchofjesuschrist.org/idp/idx/challenge")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(body)
            .send()
            .await?
            .json()
            .await?;

            // println!("challenge_response: {:?}", challenge_response);
        // Extract the state handle from the challenge response
        let state_handle = challenge_response["stateHandle"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No state handle in challenge response"))?
            .to_string();

            // println!("state_handle after challenge: {}", state_handle);

        // Send the password
        #[allow(dead_code)]
        struct PasswordResponse {
            success: SuccessResponse,
        }

        #[allow(dead_code)]
        struct SuccessResponse {
            href: String,
        }

        info!("Sending the password");
        let body = json!({
            "stateHandle": state_handle,
            "credentials": {
                "passcode": self.env.church_password
            }
        })
        .to_string();
        // println!("Password body: {}", body);

        let _challenge_answer_response: serde_json::Value = self
            .http_client
            .post("https://id.churchofjesuschrist.org/idp/idx/challenge/answer")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(body)
            .send()
            .await?
            .json()
            .await?;

        // Use the location URL from the first redirect and add okta=true
        info!("Calling /authorize using location URL with okta=true");
        if !location_url.is_empty() {
            // Parse the location URL and add okta=true parameter
            let mut authorize_url = url::Url::parse(&location_url)?;
            authorize_url.query_pairs_mut().append_pair("okta", "true");

            let authorize_res = self
                .http_client
                .get(authorize_url.as_str())
                .send()
                .await?;

            info!("Authorize Response Status: {}", authorize_res.status());
            let authorize_response = authorize_res.text().await?;
            info!("Authorize response (len={})", authorize_response.len());
        } else {
            info!("Location URL was empty, skipping /authorize call");
        }
        
        // Get the bearer token
        info!("Getting the bearer token");
        let token_res = self
            .http_client
            .get("https://referralmanager.churchofjesuschrist.org/services/auth")
            .header("Accept", "application/json")
            .send()
            .await?;

        let token_body = token_res.text().await?;
        info!("Received bearer token response (len={})", token_body.len());

        let token_json = serde_json::from_str::<serde_json::Value>(&token_body)?;
        let token = token_json["token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No 'token' key in response"))?
            .to_string();
        // println!("Raw token: {}", token);
        self.save_cookies().await?;
        self.write_bearer_token(&token).await?;

        let token = BearerToken::from_base64(token)?;
        self.bearer_token = Some(token.clone());

        Ok(token)
    }

    /// Gets the list of everyone from the referral manager. This is a HUGE request at roughly 8mb in the CSDM
    pub async fn get_people_list(&mut self) -> anyhow::Result<Vec<persons::Person>> {
        info!("Getting the people list from referral manager");
        let mut tries = 0;

        while tries < MAX_RETRIES {
            let token = match &self.bearer_token {
                Some(t) => t,
                None => &self.login().await?,
            };
            tries += 1;
            if
                let Ok(list) = self.http_client
                    .get(
                        format!(
                            "https://referralmanager.churchofjesuschrist.org/services/people/mission/{}?includeDroppedPersons=true",
                            token.claims.mission_id
                        )
                    )
                    .header("Authorization", format!("Bearer {}", token.token))
                    .send().await
            {
                if let Ok(list) = list.json::<serde_json::Value>().await {
                    let list = persons::Person::parse_lossy(list);
                    info!("Received {} people from referral manager", list.len());
                    return Ok(list);
                } else {
                    info!("Getting the people list failed at JSON parse");
                    self.bearer_token = None;
                }
            } else {
                info!("Getting the people list failed at the request");
                self.bearer_token = None;
            }
        }
        Err(anyhow::anyhow!("Max tries exceeded"))
    }

    /// Gets a cached list from referral manager to save trips to church servers.
    /// A cache will be considered 'hit' if the list is less than an hour old.
    pub async fn get_cached_people_list(&mut self) -> anyhow::Result<Vec<persons::Person>> {
        let lists_path = PathBuf::from(&self.env.working_path).join("people_lists");
        std::fs::create_dir_all(&lists_path)?;

        let now = SystemTime::now();
        let now = now
            .duration_since(UNIX_EPOCH)
            .context("Your clock is wrong")?
            .as_secs();

        // Read all the entries in the cache
        for f in std::fs::read_dir(&lists_path)? {
            match f {
                Ok(f) if f.file_type()?.is_file() => {
                    if let Ok(file_name) = f.file_name().into_string() {
                        if let Some(file_name) = file_name.split_once('.') {
                            if let Ok(timestamp) = file_name.0.parse::<u64>() {
                                if let Some(diff) = now.checked_sub(timestamp) {
                                    if diff < 60 * 60 {
                                        info!("Cache hit");
                                        return Ok(persons::Person::parse_lossy(
                                            serde_json::from_str(
                                                &std::fs::read_to_string(f.path()).unwrap(),
                                            )?,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                _ => (),
            }
        }
        info!("Cache miss");
        let list = self.get_people_list().await?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(lists_path.join(format!("{now}.json")))?;
        serde_json::to_writer(file, &json!({"persons": &list}))?;
        Ok(list)
    }

    pub async fn get_person_timeline(
        &mut self,
        person: &persons::Person,
    ) -> anyhow::Result<Vec<persons::TimelineEvent>> {
        info!("Getting timeline for {}", person.guid);
        let mut tries = 0;

        while tries < MAX_RETRIES {
            tries += 1;
            if let Ok(list) = self
                .http_client
                .get(format!(
                    "https://referralmanager.churchofjesuschrist.org/services/progress/timeline/{}",
                    person.guid
                ))
                .send()
                .await
            {
                if let Ok(list) = list.json::<serde_json::Value>().await {
                    let mut list: Vec<persons::TimelineEvent> =
                        persons::TimelineEvent::parse_lossy(list);

                    //Apply MST to EST conversion for each event
                    for event in &mut list {
                        event.convert_mst_to_est();
                    }

                    info!(
                        "Received {} timeline events from referral manager",
                        list.len()
                    );
                    return Ok(list);
                } else {
                    info!("Getting the timeline events list failed at JSON parse");
                    self.login().await?;
                }
            } else {
                info!("Getting the timeline events list failed at the request");
                self.login().await?;
            }
        }
        Err(anyhow::anyhow!("Max tries exceeded"))
    }

    pub async fn get_person_last_contact(
        &mut self,
        person: &persons::Person,
    ) -> anyhow::Result<Option<NaiveDateTime>> {
        let timeline = self.get_person_timeline(person).await?;
        for item in timeline {
            match item.item_type {
                persons::TimelineItemType::Contact | persons::TimelineItemType::Teaching => {
                    return Ok(Some(item.item_date));
                }
                persons::TimelineItemType::NewReferral => {
                    return Ok(None);
                }
                _ => {
                    continue;
                }
            }
        }
        Ok(None)
    }

    pub async fn get_person_contact_time(
        &mut self,
        person: &persons::Person,
    ) -> anyhow::Result<Option<usize>> {
        let mut timeline = self.get_person_timeline(person).await?;
        timeline.reverse();

        let mut referral_sent = None;
        let mut last_contact = None;

        for item in timeline {
            match item.item_type {
                persons::TimelineItemType::NewReferral => {
                    referral_sent = Some(item.item_date);
                    last_contact = None;
                }
                persons::TimelineItemType::Contact | persons::TimelineItemType::Teaching => {
                    if last_contact.is_none() {
                        last_contact = Some(item.item_date);
                    }
                }
                _ => {
                    continue;
                }
            }
        }

        if let Some(referral_sent) = referral_sent {
            if let Some(last_contact) = last_contact {
                let duration = last_contact.signed_duration_since(referral_sent);
                return Ok(Some(duration.num_minutes() as usize));
            }
        }
        Ok(None)
    }
}
