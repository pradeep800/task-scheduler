use crate::configuration::Config;
use health_checks::HealthCheckDb;
use sqlx::PgPool;
use std::time::Duration;
use tasks::TasksDb;
use tokio::time::sleep;
use tracing::{error, info, info_span, instrument};

pub async fn process(config: &Config) {
    let health_db_pool = config.health_check.get_pool().await;

    let task_db_pool = config.tasks.get_pool().await;
    info!("database is connected");
    loop {
        match process_iteration(&health_db_pool, &task_db_pool).await {
            Ok(_) => (),
            Err(e) => error!("Error during processing iteration: {}", e),
        }
        sleep(Duration::from_secs(1)).await;
    }
}
#[instrument(level = "info")]
async fn process_iteration(
    health_db_pool: &PgPool,
    task_db_pool: &PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut health_transaction = health_db_pool.begin().await?;
    let mut task_transaction = task_db_pool.begin().await?;
    info!("transaction created");
    let health_entries = HealthCheckDb::get_10_dead_health_entries(&mut health_transaction).await?;

    for x in health_entries {
        let current_task =
            TasksDb::get_lock_with_id(&mut task_transaction, x.task_id as i32).await?;
        let span = info_span!("got task with tracing_id {}", current_task.tracing_id);
        let _guard = span.enter();

        HealthCheckDb::worker_finished(&mut health_transaction, x.task_id, x.pod_name.as_str())
            .await?;

        if current_task.total_retry > current_task.current_retry {
            info!("total task > current task");
            TasksDb::increase_current_retry_add_failed_and_is_producible_true(
                &mut task_transaction,
                current_task.id,
                "Heartbeat delay",
            )
            .await?;
        } else {
            info!("total task = current task");
            TasksDb::add_failed_with_trans(
                &mut task_transaction,
                current_task.id,
                "Heartbeat delay",
            )
            .await?;
        }
    }

    health_transaction.commit().await?;
    task_transaction.commit().await?;
    info!("committed all changes to  databases");
    Ok(())
}
