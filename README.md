# wajob

A dead simple distributed job scheduler for micro-services, with support for single job, interval job and cron job.

## Design

* Lightweight Scheduler: job registry, job status, job management.
* Flexible Executor: as a standalone service or as a library of a service.
* Common Stacks: [NATS](https://github.com/nats-io/nats.rs), [Redis](https://github.com/mitsuhiko/redis-rs), [Tokio](https://github.com/tokio-rs/tokio), [Tonic](https://github.com/hyperium/tonic)
* GRPC: Every executor registers its grpc methods to the scheduler which could be called by the scheduler.

## Features

* [ ] Create/Read/Update/Delete jobs to run.
* [ ] Supports one time execution and repetitive executions triggered at a fixed interval.
* [ ] Jobs are persisted.
* [ ] The system is scalable to thousands of jobs and many workers.
* [ ] Monitoring, such as timeout, status.
* [ ] Retry with a maximum times.
