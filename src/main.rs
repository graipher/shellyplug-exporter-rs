use std::env;
use std::time::{Duration, SystemTime};

use prometheus_exporter::{
    self,
    prometheus::register_gauge,
    prometheus::register_gauge_vec,
};
use reqwest::Client;
use serde::Deserialize;
use serde_json::from_str;

#[derive(Debug, Deserialize)]
struct Aenergy {
    total: f64,
}

#[derive(Debug, Deserialize)]
struct Temperature {
    #[serde(rename = "tC")]
    t_c: f64,
}

#[derive(Debug, Deserialize)]
struct Switch0 {
    output: bool,
    apower: f64,
    voltage: f64,
    current: f64,
    aenergy: Aenergy,
    temperature: Temperature,
}

#[derive(Debug, Deserialize)]
struct Stable {
    version: String,
}

#[derive(Debug, Deserialize)]
struct AvailableUpdates {
    stable: Option<Stable>,
}

#[derive(Debug, Deserialize)]
struct Sys {
    mac: String,
    available_updates: AvailableUpdates,
}

#[derive(Debug, Deserialize)]
struct ShellyplugResponse {
    #[serde(rename = "switch:0")]
    switch0: Switch0,
    sys: Sys,
}


async fn get_data(client: &Client, url: &String) -> Result<ShellyplugResponse, reqwest::Error> {
    let response = client.get(url).send().await?;
    let json = response.json::<ShellyplugResponse>().await?;
    Ok(json)
}

#[tokio::main]
async fn main() {
    let port = env::var("PORT").or::<String>(Ok("9185".to_string())).unwrap();
    let period = Duration::from_secs(from_str::<u64>(&env::var("PERIOD").or::<String>(Ok("60".to_string())).unwrap()).unwrap());
    let binding = format!("0.0.0.0:{}", port).parse().unwrap();
    println!("Listening on {}", binding);
    println!("Updating every {:?}", period);
    let exporter = prometheus_exporter::start(binding).unwrap();

    let client = Client::new();
    let url = format!("{}/rpc/Shelly.GetStatus", env::var("SHELLYPLUG_URL").expect("SHELLYPLUG_URL not set"));

    let a_power = register_gauge_vec!("shellyplug_apower", "Instantaneous power in W", &["mac"]).unwrap();
    let voltage_shelly = register_gauge_vec!("shellyplug_voltage", "Voltage in V", &["mac"]).unwrap();
    let current_shelly = register_gauge_vec!("shellyplug_current", "Current in A", &["mac"]).unwrap();
    let a_energy_total_shelly = register_gauge_vec!("shellyplug_aenergy_total", "Total energy so far in Wh", &["mac"]).unwrap();
    let temperature_shelly = register_gauge_vec!("shellyplug_temperature", "Temperature of Shellyplug in °C", &["mac"]).unwrap();
    let output_shelly = register_gauge_vec!("shellyplug_output", "true if output channel is currently on, false otherwise", &["mac"]).unwrap();
    let available_updates_shelly = register_gauge_vec!("shellyplug_available_updates_info", "Information about available updates", &["mac", "version"]).unwrap();
    let last_updated_shelly = register_gauge_vec!("shellyplug_last_updated", "Last update of Shellyplug", &["mac"]).unwrap();
    let process_start_time = register_gauge!("process_start_time_seconds", "Start time of the process").unwrap();

    let mut now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    process_start_time.set(now as f64);

    loop {
        match get_data(&client, &url).await {
            Ok(data) => {
                // println!("{:?}", data);
                now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
                let mac = data.sys.mac;
                a_power.get_metric_with_label_values(&[&mac]).unwrap().set(data.switch0.apower);
                voltage_shelly.get_metric_with_label_values(&[&mac]).unwrap().set(data.switch0.voltage);
                current_shelly.get_metric_with_label_values(&[&mac]).unwrap().set(data.switch0.current);
                a_energy_total_shelly.get_metric_with_label_values(&[&mac]).unwrap().set(data.switch0.aenergy.total);
                temperature_shelly.get_metric_with_label_values(&[&mac]).unwrap().set(data.switch0.temperature.t_c);
                if data.switch0.output {
                    output_shelly.get_metric_with_label_values(&[&mac]).unwrap().set(1.);
                } else {
                    output_shelly.get_metric_with_label_values(&[&mac]).unwrap().set(0.);
                }
                available_updates_shelly.reset();
                match data.sys.available_updates.stable {
                    Some(v) => available_updates_shelly.get_metric_with_label_values(&[&mac, &v.version]).unwrap().set(1.),
                    None => available_updates_shelly.get_metric_with_label_values(&[&mac, "current"]).unwrap().set(1.)
                }
                last_updated_shelly.get_metric_with_label_values(&[&mac]).unwrap().set(now as f64);
            }
            Err(err) => eprintln!("{}", err)
        }
        let _guard = exporter.wait_duration(period);
    }
}
