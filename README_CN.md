# job-center

一个最简针对微服务的任务调度服务，支持一次执行，以及周期多次执行。

 > Rust 来实现原型代码还是有点麻烦。目前还是个半成品，后续有时间慢慢完善。

## 设计

* 逻辑上主要分成两个组件，scheduler(调度节点) 和 executor(工作节点), executor 节点将自己的 grpc 方法注册到 scheduler. 注：时间关系，目前 executor 暂未实现，在 scheduler 中使用 shell 命令来代替。
* scheduler 服务功能包括 executor 节点的服务注册，以及心跳检测，任务管理 (包括任务 CRUD，状态查询，任务唯一任务 ID 生成来关联采集的日志以及 tracing).
* scheduler 提供 grpc admin 接口，提供给客户端进行管理任务。
