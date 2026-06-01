use parking_lot::Mutex;
use std::sync::Arc;

use async_trait::async_trait;
use peri_agent::{agent::state::State, middleware::r#trait::Middleware, tools::BaseTool};

use super::{
    tools::{CronListTool, CronRegisterTool, CronRemoveTool},
    CronScheduler,
};

/// Cron 中间件：提供 cron_register / cron_list / cron_remove 工具
pub struct CronMiddleware {
    scheduler: Arc<Mutex<CronScheduler>>,
}

impl CronMiddleware {
    pub fn new(scheduler: Arc<Mutex<CronScheduler>>) -> Self {
        Self { scheduler }
    }
}

#[async_trait]
impl<S: State> Middleware<S> for CronMiddleware {
    fn name(&self) -> &str {
        "CronMiddleware"
    }

    fn collect_tools(&self, _cwd: &str) -> Vec<Box<dyn BaseTool>> {
        let sched = self.scheduler.clone();
        vec![
            Box::new(CronRegisterTool::new(sched.clone())),
            Box::new(CronListTool::new(sched.clone())),
            Box::new(CronRemoveTool::new(sched)),
        ]
    }
}
