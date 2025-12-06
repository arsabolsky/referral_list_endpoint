//Karter Arritt
use reqwest::Client;
use serde_json::Value;

pub async fn send_to_google_apps_script(
    body: Value,
    endpoint_url: String
) -> Result<String, Box<dyn std::error::Error>> {
    let client = Client::new();

    // Append static query parameters to the endpoint URL
    let endpoint_url_with_params = format!("{}?location=ReferralScore&operation=replace", endpoint_url);
    
    // Send POST request
    let res = client.post(endpoint_url_with_params).json(&body).send().await?;

    // Check for successful response
    if res.status().is_success() {
        // Parse the response JSON (assuming it's a decrypted object)
        let response_text = res.text().await?;
        Ok(response_text)
    } else {
        Err(format!("Request failed with status: {}", res.status()).into())
    }
}
