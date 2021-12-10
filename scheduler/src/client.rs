use api::cronjob_admin_client::CronjobAdminClient;
use api::{JobType};
use clap::Parser;
use log::LevelFilter;
use uuid::Uuid;

/// api is the namespace for the generated GRPC code.
pub mod api {
    tonic::include_proto!("api");
}

/// scheduler-client is a client to interact with the scheduler-server.
#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug)]
enum SubCommand {
    Submit(Submit),
    Status(Status),
    Logs(Logs),
    Kill(Kill),
}

/// Submit a job to the server
#[derive(Parser, Debug)]
struct Submit {
    /// Command to submit
    #[clap(short, long, default_value = "ls")]
    command: String,
    /// Job type, single or interval
    #[clap(short, long, default_value = "single")]
    jtype: String,
    /// interval,such as 5s, 1m, 1h
    #[clap(short, long, default_value = "5s")]
    interval: String,
}

/// Query the status of a job
#[derive(Parser, Debug)]
struct Status {
    /// The job_id to fetch the status for
    job_id: String,
}

/// Fetch the logs for a job
#[derive(Parser, Debug)]
struct Logs {
    /// JobId to fetch the logs for
    job_id: String,
}

/// Kill a job
#[derive(Parser, Debug)]
struct Kill {
    /// JobId to kill
    job_id: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::builder().filter_level(LevelFilter::Info).init();

    let opts: Opts = Opts::parse();

    let mut client = CronjobAdminClient::connect("http://127.0.0.1:50051".to_string()).await?;

    match opts.subcmd {
        SubCommand::Submit(s) => {
            log::info!("Submitting command {}", s.command);
            let job_type = if s.jtype == "single" {
                JobType::Single
            } else {
                JobType::Interval
            };
            let request = tonic::Request::new(api::Command {
              command: s.command,
              job_type: job_type as i32,
              interval: s.interval,
            });
            let response = client.submit(request).await?;

            log::info!("response {:?}", response)
        }
        SubCommand::Status(s) => {
            log::info!("Getting status for job_id {}", &s.job_id);

            let request = tonic::Request::new(api::JobId { id: s.job_id.to_owned() });
            let response = client.status(request).await?;

            log::info!("response {:?}", response)
        }
        SubCommand::Logs(s) => {
            let uuid = parse_uuid_or_abort(s.job_id);
            log::info!("Fetching logs for job_id {}", uuid)
        }
        SubCommand::Kill(s) => {
            log::info!("Killing job_id {}", &s.job_id);

            let request = tonic::Request::new(api::JobId { id: s.job_id.to_owned() });
            let response = client.stop(request).await?;

            log::info!("response {:?}", response)
        }
    };

    Ok(())
}

/// Parse the given UUID or abort the process if parsing fails.
fn parse_uuid_or_abort(input: String) -> Uuid {
    match Uuid::parse_str(input.as_str()) {
        Err(e) => {
            log::error!("Error parsing UUID: {}", e);
            std::process::exit(1)
        }
        Ok(uuid) => uuid,
    }
}
