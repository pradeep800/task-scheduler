use common::tracing::{get_subscriber, init_subscriber};
use core::panic;
use reqwest;
use reqwest::header::{HeaderMap, HeaderValue};
use std::env;
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::time::sleep;
use tokio::{process::Command, time::interval};
use tracing::Instrument;
use tracing::{error, info, info_span, instrument};
const HEART_BEAT_INTERVAL_IN_SECOND: i32 = 5;
const URL: &str = "http://status-check-svc:80";

#[derive(Debug)]
struct ChannelBody {
    pub status: String,
    pub body: Option<String>,
}

#[tokio::main]
async fn main() {
    let subscriber = get_subscriber("worker".to_string(), "info".to_string(), std::io::stdout);
    init_subscriber(subscriber);

    let tracing_id = env::var("tracing_id").unwrap();
    let signed_url = env::var("signed_url").unwrap();
    let host_id = env::var("host_id").unwrap();
    let jwt = env::var("jwt")
        .unwrap()
        .trim_matches('"')
        .trim_end_matches('\n')
        .to_string();
    info!("all env parsed correctly");

    let info_span = info_span!(
        "worker info span with tracing_id {} and host_id {}",
        tracing_id,
        host_id
    );
    let _guard = info_span.enter();
    let jwt: Arc<String> = Arc::new(jwt);

    download_file_from_signed_url(&signed_url, "./task")
        .await
        .unwrap();
    info!("file is downloaded successfully");
    Command::new("chmod")
        .args(["+x", "./task"])
        .output()
        .await
        .unwrap();
    info!("added +x permissoin to file");
    let heartbeat_interval = Duration::from_secs(HEART_BEAT_INTERVAL_IN_SECOND as u64);
    let jsonwebtoken = Arc::clone(&jwt);
    //keep sending heartbeat
    let heartbeat_task = tokio::spawn(
        async move {
            let mut interval = interval(heartbeat_interval);
            let mut i = 0;
            loop {
                info!("sending heartbeats");
                match send_heartbeat(&jsonwebtoken).await {
                    Ok(_) => {
                        i = 0;
                    }
                    Err(_) => {
                        error!("sending heartbeat {}th time", i);
                        i += 1;
                        if i == 4 {
                            error_with_panic("Can't send message to channel").await;
                        }
                    }
                }
                interval.tick().await;
            }
        }
        .in_current_span(),
    );
    //waiting for atleast one heart is sended to coordinator
    sleep(std::time::Duration::from_secs(2)).await;

    //TODO: it should not go beond this line until one heart beat is sended
    let binary_task_execution = execute_binary();

    tokio::select! {
        task_body= binary_task_execution => {
           send_status(&task_body, &Arc::clone(&jwt)).await;
        },
       _ = heartbeat_task => {
           send_status(&ChannelBody{
                status:"FAILED".to_string(),
                body:Some("Can't send health check".to_string()),
            }, &Arc::clone(&jwt)).await;

        },
    };
}

#[instrument(level = "info")]
async fn send_heartbeat(jwt: &Arc<String>) -> Result<(), reqwest::Error> {
    let jwt = format!("{}", jwt);
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        HeaderValue::from_str(&jwt).unwrap(),
    );
    let client = reqwest::Client::new();
    let _ = client
        .get(format!("{}/worker/heart-beat", URL))
        .headers(headers)
        .send()
        .await?;
    info!("heart beat sended");
    Ok(())
}
#[instrument(level = "info")]
async fn send_status(cb: &ChannelBody, jwt: &Arc<String>) {
    let client = reqwest::Client::new();

    let mut i: u32 = 1;
    let two: u32 = 2;
    if cb.status == "SUCCESS" {
        loop {
            let mut headers = HeaderMap::new();
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(&jwt).unwrap(),
            );
            let res = client
                .post(format!("{}/worker/update-status", URL))
                .headers(headers)
                .json(&serde_json::json!({"status":"SUCCESS"}))
                .send()
                .await
                .unwrap();

            if res.status() == 200 {
                info!("status updated successfully (status:success)");
                break;
            }
            if i == 4 {
                error_with_panic("Can't able to send status in 3 try").await;
            }
            info!("trying to send success to status check {}th time", i);
            let time = two.pow(i);
            sleep(Duration::from_secs(time as u64)).await;

            i += 1;
        }
    } else {
        loop {
            let mut headers = HeaderMap::new();
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(&jwt).unwrap(),
            );
            let res = client
                .post(format!("{}/worker/update-status", URL))
                .json(&serde_json::json!({"status":"FAILED","failed_reason":cb.body}))
                .headers(headers)
                .send()
                .await
                .unwrap();
            if res.status() == 200 {
                info!("status updated successfully (status:failure)");
                break;
            }
            if i == 4 {
                error_with_panic("Can't able to send status in 3 try").await;
            }
            info!("trying to send failed to status check {}th time", i);
            let time = two.pow(i);
            sleep(Duration::from_secs(time as u64)).await;
            i += 1;
        }
    }
}

#[instrument(level = "error")]
async fn error_with_panic(message: &str) {
    error!(message);
    sleep(Duration::from_secs(2)).await;
    panic!("{}", message);
}

#[instrument(level = "error")]
async fn download_file_from_signed_url(
    signed_url: &str,
    output_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let response = client.get(signed_url).send().await?;
    if !response.status().is_success() {
        return Err(format!("Failed to download file: HTTP {}", response.status()).into());
    }

    let content = response.bytes().await?;

    let mut file = File::create(output_path).await.unwrap();

    file.write_all(&content).await?;

    Ok(())
}

#[instrument(level = "info")]
async fn execute_binary() -> ChannelBody {
    let mut task_status: Option<ExitStatus> = None;

    if let Ok(status) = Command::new("./task").status().await {
        if status.success() {
            return ChannelBody {
                status: "SUCCESS".to_string(),
                body: None,
            };
        }
        task_status = Some(status);
    }

    let status = task_status.and_then(|s| s.code()).unwrap_or(0);
    let status_message = if status == 0 {
        format!("Task status code is not {}", status)
    } else {
        format!("Task status code is {}", status)
    };

    return ChannelBody {
        status: "FAILED".to_string(),
        body: Some(status_message),
    };
}
