use std::collections::HashMap;
use std::io::Write;
use std::pin::Pin;
use std::process::Command;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use api::cronjob_admin_server::{CronjobAdmin, CronjobAdminServer};
use api::{JobId, JobType, StatusResponse, StatusType};
use clap::Parser;
use futures::Stream;
use humantime::parse_duration;
use log::LevelFilter;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use uuid::Uuid;
use wait_timeout::ChildExt;

/// scheduler-server runs arbitrary Linux commands or call grpc services.
#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug)]
enum SubCommand {
    ExecTest(ExecTest),
    Serve(Serve),
}

/// Test the execution of the given command
#[derive(Parser, Debug)]
struct ExecTest {
    /// Command to execute
    command: String,
}

/// Server requests
#[derive(Parser, Debug)]
struct Serve {}

/// api is the namespace for the GRPC generated code.
pub mod api {
    tonic::include_proto!("api");
}

/// WorkerService handles the GRPC requests.
#[derive(Debug)]
pub struct WorkerService {
    /// JobManager manages the actual jobs submitted by the client.
    job_manager: JobManager,
}

#[tonic::async_trait]
impl CronjobAdmin for WorkerService {
    /// Submit a request to run the given command and return the UUID of the resulting job
    async fn submit(&self, request: Request<api::Command>) -> Result<Response<JobId>, Status> {
        log::info!("Got a request: {:?}", request.get_ref());
        let job_type = if request.get_ref().job_type == 0 {
            JobType::Single
        } else {
            JobType::Interval
        };

        let result = self.job_manager.submit(Job {
            id: Uuid::new_v4(),
            command: request.get_ref().command.clone(),
            status: StatusResponse {
                status: StatusType::Running as i32,
                exit_code: 0,
            },
            job_type,
            interval: request.get_ref().interval.clone(),
        });

        match result {
            Ok(uuid) => Ok(Response::new(api::JobId {
                id: uuid.to_string(),
            })),
            Err(e) => Err(Status::new(tonic::Code::Internal, e)),
        }
    }

    /// Stop the job identified by the given UUID
    async fn stop(
        &self,
        request: tonic::Request<JobId>,
    ) -> Result<tonic::Response<api::Empty>, tonic::Status> {
        log::info!("stopping {:?}", request.get_ref());

        let parsed = Uuid::parse_str(&request.get_ref().id);
        if parsed.is_err() {
            return Err(Status::invalid_argument("invalid UUID"));
        }
        let result = self.job_manager.kill(parsed.unwrap());
        match result {
            Ok(_) => Ok(Response::new(api::Empty {})),
            Err(e) => Result::Err(Status::new(tonic::Code::Internal, e)),
        }
    }

    /// Query the status of the job identified by the given UUID
    async fn status(
        &self,
        request: tonic::Request<JobId>,
    ) -> Result<tonic::Response<StatusResponse>, tonic::Status> {
        log::info!("status {:?}", request.get_ref());

        let parsed = Uuid::parse_str(&request.get_ref().id);
        if parsed.is_err() {
            return Err(Status::invalid_argument("invalid UUID"));
        }
        let result = self.job_manager.status(parsed.unwrap());
        match result {
            Ok(s) => Ok(Response::new(s)),
            Err(e) => Result::Err(Status::new(tonic::Code::Internal, e)),
        }
    }

    type GetLogsStream =
        Pin<Box<dyn Stream<Item = Result<api::Log, Status>> + Send + Sync + 'static>>;

    /// Stream the logs from the job identified by the given UUID
    async fn get_logs(
        &self,
        request: tonic::Request<JobId>,
    ) -> Result<tonic::Response<Self::GetLogsStream>, tonic::Status> {
        log::info!("get_logs {:?}", request.get_ref());
        Result::Err(Status::unimplemented("get_logs is not yet implemented"))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::builder()
        .filter_level(LevelFilter::Info)
        .format_timestamp(Some(env_logger::TimestampPrecision::Millis))
        // This custom formatter allows us to include the PID in the logs for easier debugging.
        .format(|buf, record| {
            let ts = buf.timestamp();
            let p = std::process::id();
            writeln!(
                buf,
                "[{} {} {} {}] {}",
                ts,
                record.level(),
                p,
                record.module_path().unwrap(),
                record.args()
            )
        })
        .init();

    let opts: Opts = Opts::parse();

    match opts.subcmd {
        SubCommand::ExecTest(e) => {
            log::info!("re-exec test {:?}", e.command);

            let id = Uuid::new_v4();

            let job = Job {
                id,
                status: StatusResponse {
                    status: StatusType::Running as i32,
                    exit_code: 0,
                },
                job_type: JobType::Single,
                interval: String::from("5s"),
                command: e.command,
            };
            log::info!("Created job {:?}", &job);

            run_single(&job).unwrap();
        }
        SubCommand::Serve(_) => {
            log::info!("Serving admin server at: 0.0.0.0:50051");

            let addr = "0.0.0.0:50051".parse()?;
            let worker = WorkerService {
                job_manager: JobManager::new(),
            };

            Server::builder()
                .add_service(CronjobAdminServer::new(worker))
                .serve(addr)
                .await?;
        }
    }

    Ok(())
}

fn run_single(job: &Job) -> Option<i32> {
    let timeout = Duration::from_secs(60);
    log::info!(
        "{:?} running command job: {:?}",
        hostname::get().unwrap(),
        job
    );
    let mut child = Command::new(job.command.as_str()).spawn().unwrap();
    match child.wait_timeout(timeout).unwrap() {
        Some(status) => status.code(),
        None => {
            log::info!("{:?} timed out, job: {:?}", hostname::get().unwrap(), job);
            child.kill().unwrap();
            child.wait().unwrap().code()
        }
    }
}

/// Job represents a command that has been requested to run by a client.
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Job {
    /// id uniquely identifies the job
    id: Uuid,

    /// status contains the current status of the job
    status: StatusResponse,

    /// job type, single or interval
    job_type: JobType,

    /// interval for interval job
    interval: String,

    /// command is the command that was requested. It's a String (rather than a &str) because the Job owns the content.
    command: String,
}

/// JobManager manages the submitted jobs.
#[derive(Debug)]
struct JobManager {
    jobs: Mutex<HashMap<Uuid, Job>>,
}

impl JobManager {
    /// Return a new JobManager with an empty jobs map.
    fn new() -> JobManager {
        JobManager {
            jobs: Mutex::new(HashMap::new()),
        }
    }

    /// Submit the given command to be executed
    fn submit(&self, j: Job) -> Result<Uuid, &str> {
        let id = Uuid::new_v4();
        let mut guard = self.jobs.lock().unwrap();

        let mut job = Job {
            id,
            job_type: j.job_type as JobType,
            interval: j.interval.clone(),
            status: StatusResponse {
                status: StatusType::Running as i32,
                exit_code: 0,
            },
            command: j.command.clone(),
        };
        log::info!("Created job {:?}", &job);

        match job.job_type {
            JobType::Single => {
                job.status.exit_code = run_single(&job).unwrap();
                if job.status.exit_code == 0 {
                    job.status.status = StatusType::Completed as i32;
                } else {
                    job.status.status = StatusType::Timeout as i32;
                }
                guard.insert(id, job);
            }
            JobType::Interval => {
                guard.insert(id, job.clone());
                thread::spawn(move || loop {
                    thread::sleep(parse_duration(&job.interval).unwrap());
                    job.status.exit_code = run_single(&job).unwrap();
                    if job.status.exit_code == 0 {
                        job.status.status = StatusType::Completed as i32;
                    } else {
                        job.status.status = StatusType::Timeout as i32;
                    }
                });
            }
        }

        Ok(id)
    }

    /// Return the status of the job identified by the given UUID
    fn status(&self, uuid: Uuid) -> Result<StatusResponse, &str> {
        let guard = self.jobs.lock().unwrap();

        let command = guard.get(&uuid);
        match command {
            Some(job) => Ok(job.status.clone()),
            None => Err("Job not found"),
        }
    }

    /// Kill the job identified by the given UUID
    fn kill(&self, uuid: Uuid) -> Result<(), &str> {
        let mut guard = self.jobs.lock().unwrap();

        let command = guard.get_mut(&uuid);
        match command {
            Some(mut job) => {
                job.status = api::StatusResponse {
                    status: StatusType::Stopped as i32,
                    exit_code: 0,
                };
                Ok(())
            }
            None => Err("Job not found"),
        }
    }
}
