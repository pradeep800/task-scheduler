use common::{database::Database, sqs::SQS};

use config::{File, FileFormat};
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub database: Database,
    pub sqs: SQS,
}

pub fn get_configuration() -> Config {
    config::Config::builder()
        .add_source(File::new("env.yaml", FileFormat::Yaml))
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap()
}
