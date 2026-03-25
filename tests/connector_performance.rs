#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(unexpected_cfgs)]
#![allow(clippy::uninlined_format_args)]

//! Connector 框架性能测试
//!
//! 测试内容：
//! - 并发注册/查询性能
//! - 超时控制性能
//! - 内存使用检测

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::{Value, json};
use vault_service::connector::{
    Connector, ConnectorConfig, ConnectorError, ConnectorRegistry, ConnectorResult, TimeoutConfig,
    TimeoutWrapper, ValidatedParams, ValidationError,
};

// ============================================================================
// 测试辅助类型
// ============================================================================

/// 轻量级测试连接器
struct PerfTestConnector {
    name: &'static str,
}

#[async_trait]
impl Connector for PerfTestConnector {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        "Performance test connector"
    }

    async fn init(&mut self, _config: ConnectorConfig) -> ConnectorResult<()> {
        Ok(())
    }

    async fn validate(&self, params: &Value) -> Result<ValidatedParams, ValidationError> {
        Ok(ValidatedParams::new(params.clone()))
    }

    async fn execute(&self, params: ValidatedParams) -> ConnectorResult<Value> {
        Ok(params.into_json())
    }

    async fn cleanup(&self) -> ConnectorResult<()> {
        Ok(())
    }
}

/// 测量执行时间的辅助函数
fn measure_time<F: FnOnce() -> R, R>(f: F) -> (R, Duration) {
    let start = Instant::now();
    let result = f();
    (result, start.elapsed())
}

/// 测量异步执行时间的辅助函数
async fn measure_time_async<F, Fut, R>(f: F) -> (R, Duration)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = R>,
{
    let start = Instant::now();
    let result = f().await;
    (result, start.elapsed())
}

// ============================================================================
// 1. 并发注册/查询性能测试
// ============================================================================

#[tokio::test]
async fn test_concurrent_registration_performance() {
    println!("\n=== 并发注册性能测试 ===");

    let registry = Arc::new(ConnectorRegistry::new());
    let connector_count = 1000;
    let concurrency = 100;

    let start = Instant::now();

    // 分批并发注册
    for batch in 0..(connector_count / concurrency) {
        let mut handles = vec![];

        for i in 0..concurrency {
            let registry = Arc::clone(&registry);
            let idx = batch * concurrency + i;
            let handle = tokio::spawn(async move {
                let name = format!("connector-{}", idx);
                let name: &'static str = Box::leak(name.into_boxed_str());
                let connector = Box::new(PerfTestConnector { name });
                registry.register(name, connector).await
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap().unwrap();
        }
    }

    let elapsed = start.elapsed();
    let ops_per_sec = connector_count as f64 / elapsed.as_secs_f64();

    println!("注册 {} 个连接器耗时: {:?}", connector_count, elapsed);
    println!("平均吞吐量: {:.2} 操作/秒", ops_per_sec);
    println!(
        "平均延迟: {:.3} ms/操作",
        elapsed.as_millis() as f64 / connector_count as f64
    );

    assert_eq!(registry.len().await, connector_count);

    // 验证所有连接器可访问
    let start = Instant::now();
    for i in 0..connector_count {
        let name = format!("connector-{}", i);
        assert!(registry.get(&name).await.is_ok());
    }
    let elapsed = start.elapsed();

    println!("查询 {} 个连接器耗时: {:?}", connector_count, elapsed);
    println!(
        "平均查询延迟: {:.3} ms",
        elapsed.as_millis() as f64 / connector_count as f64
    );
}

#[tokio::test]
async fn test_concurrent_query_performance() {
    println!("\n=== 并发查询性能测试 ===");

    let registry = Arc::new(ConnectorRegistry::new());

    // 预注册一些连接器
    let connector_count = 100;
    for i in 0..connector_count {
        let name = format!("conn-{}", i);
        let name: &'static str = Box::leak(name.into_boxed_str());
        let connector = Box::new(PerfTestConnector { name });
        registry.register(name, connector).await.unwrap();
    }

    // 并发查询测试
    let query_count = 10000;
    let concurrency = 200;
    let queries_per_task = query_count / concurrency;

    let start = Instant::now();
    let mut handles = vec![];

    for task_id in 0..concurrency {
        let registry = Arc::clone(&registry);
        let handle = tokio::spawn(async move {
            let mut success_count = 0;
            for i in 0..queries_per_task {
                let idx = (task_id * queries_per_task + i) % connector_count;
                let name = format!("conn-{}", idx);
                if registry.get(&name).await.is_ok() {
                    success_count += 1;
                }
            }
            success_count
        });
        handles.push(handle);
    }

    let total_success: usize = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .sum();

    let elapsed = start.elapsed();
    let ops_per_sec = query_count as f64 / elapsed.as_secs_f64();

    println!("并发查询 {} 次耗时: {:?}", query_count, elapsed);
    println!("查询吞吐量: {:.2} 操作/秒", ops_per_sec);
    println!(
        "成功率: {}%",
        (total_success as f64 / query_count as f64) * 100.0
    );

    assert_eq!(total_success, query_count);
}

#[tokio::test]
async fn test_registry_list_performance() {
    println!("\n=== 列表操作性能测试 ===");

    let registry = Arc::new(ConnectorRegistry::new());

    // 注册不同数量的连接器进行测试
    let test_sizes = vec![10, 100, 500, 1000];

    for size in test_sizes {
        // 清空并重新注册
        registry.clear().await;

        for i in 0..size {
            let name = format!("conn-{}", i);
            let name: &'static str = Box::leak(name.into_boxed_str());
            let connector = Box::new(PerfTestConnector { name });
            registry.register(name, connector).await.unwrap();
        }

        // 测试 list_all
        let (_, elapsed) = measure_time_async(|| async {
            for _ in 0..100 {
                let _ = registry.list_all().await;
            }
        })
        .await;

        let avg_latency = elapsed.as_millis() as f64 / 100.0;
        println!("list_all ({} items): avg {:.3} ms", size, avg_latency);

        // 测试 list_all_with_description
        let (_, elapsed) = measure_time_async(|| async {
            for _ in 0..100 {
                let _ = registry.list_all_with_description().await;
            }
        })
        .await;

        let avg_latency = elapsed.as_millis() as f64 / 100.0;
        println!(
            "list_all_with_description ({} items): avg {:.3} ms",
            size, avg_latency
        );
    }
}

// ============================================================================
// 2. 超时控制性能测试
// ============================================================================

#[tokio::test]
async fn test_timeout_overhead() {
    println!("\n=== 超时控制开销测试 ===");

    let iterations = 10000;

    // 基准测试：无超时
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = async { Ok::<i32, ConnectorError>(42) }.await;
    }
    let baseline = start.elapsed();

    // 启用超时
    let config = TimeoutConfig::new().with_default_timeout(30);
    let wrapper = TimeoutWrapper::new("test", config);

    let start = Instant::now();
    for _ in 0..iterations {
        let _: Result<i32, ConnectorError> =
            wrapper.run(async { Ok::<_, ConnectorError>(42) }).await;
    }
    let with_timeout = start.elapsed();

    let overhead = with_timeout.as_micros() as f64 - baseline.as_micros() as f64;
    let overhead_per_op = overhead / iterations as f64;

    println!("基准测试 (无超时): {:?}", baseline);
    println!("启用超时: {:?}", with_timeout);
    println!("总开销: {:.2} μs", overhead);
    println!("每次操作开销: {:.3} μs", overhead_per_op);

    // 超时开销应该小于 100μs
    assert!(
        overhead_per_op < 100.0,
        "超时开销过高: {:.3} μs",
        overhead_per_op
    );
}

#[tokio::test]
async fn test_timeout_accuracy() {
    println!("\n=== 超时精度测试 ===");

    // 使用秒级超时测试，因为 Tokio 的 sleep 精度在毫秒级时不够准确
    let test_timeouts = vec![1, 2]; // seconds
    let tolerance = 500; // 允许 500ms 误差

    for timeout_secs in test_timeouts {
        let config = TimeoutConfig::new()
            .with_min_timeout(100)
            .with_default_timeout(timeout_secs as u64);
        let wrapper = TimeoutWrapper::new("test", config);

        let start = Instant::now();
        let result = wrapper
            .run(async {
                tokio::time::sleep(Duration::from_secs(timeout_secs as u64 * 2)).await;
                Ok::<_, ConnectorError>(())
            })
            .await;

        let elapsed = start.elapsed();
        let elapsed_ms = elapsed.as_millis() as i64;
        let expected_ms = (timeout_secs * 1000) as i64;
        let deviation = (elapsed_ms - expected_ms).abs();

        assert!(result.is_err(), "应该超时");
        assert!(
            deviation < tolerance as i64,
            "超时精度偏差过大: 期望 {}ms, 实际 {}ms, 偏差 {}ms",
            expected_ms,
            elapsed_ms,
            deviation
        );

        println!(
            "超时 {}s: 实际 {}ms, 偏差 {}ms",
            timeout_secs, elapsed_ms, deviation
        );
    }
}

// ============================================================================
// 3. 内存使用检测
// ============================================================================

#[tokio::test]
async fn test_memory_usage_with_many_connectors() {
    println!("\n=== 内存使用测试 ===");

    // 此测试需要系统支持，使用简单的容量测试
    let registry = Arc::new(ConnectorRegistry::new());

    let test_sizes = vec![100, 500, 1000, 5000];

    for size in test_sizes {
        // 清空
        registry.clear().await;

        // 强制垃圾回收提示
        #[cfg(feature = "jemalloc")]
        unsafe {
            jemalloc_ctl::epoch::advance().unwrap();
        }

        let start = Instant::now();

        // 注册指定数量的连接器
        for i in 0..size {
            let name = format!("memory-test-connector-{}", i);
            let name: &'static str = Box::leak(name.into_boxed_str());
            registry
                .register(name, Box::new(PerfTestConnector { name }))
                .await
                .unwrap();
        }

        let elapsed = start.elapsed();

        // 验证所有连接器都注册成功
        assert_eq!(registry.len().await, size);

        println!("注册 {} 个连接器完成，耗时 {:?}", size, elapsed);

        // 随机查询验证内存可访问性
        let sample_size = (size / 10).max(10);
        for i in (0..size).step_by(size / sample_size) {
            let name = format!("memory-test-connector-{}", i);
            assert!(registry.get(&name).await.is_ok());
        }

        println!("随机抽样 {} 次查询全部成功", sample_size);
    }
}

#[tokio::test]
async fn test_registry_clear_performance() {
    println!("\n=== 注册表清理性能测试 ===");

    let registry = Arc::new(ConnectorRegistry::new());

    // 注册大量连接器
    let connector_count = 5000;
    for i in 0..connector_count {
        let name = format!("cleanup-test-{}", i);
        let name: &'static str = Box::leak(name.into_boxed_str());
        let connector = Box::new(PerfTestConnector { name });
        registry.register(name, connector).await.unwrap();
    }

    assert_eq!(registry.len().await, connector_count);

    // 测量清理时间
    let (_, elapsed) = measure_time_async(|| registry.clear()).await;

    println!("清理 {} 个连接器耗时: {:?}", connector_count, elapsed);
    println!(
        "平均清理速度: {:.2} 连接器/秒",
        connector_count as f64 / elapsed.as_secs_f64()
    );

    assert!(registry.is_empty().await);
}

// ============================================================================
// 4. 并发执行性能测试
// ============================================================================

#[tokio::test]
async fn test_concurrent_execute_throughput() {
    println!("\n=== 并发执行吞吐量测试 ===");

    let registry = Arc::new(ConnectorRegistry::new());

    // 注册多个连接器
    let connector_count = 10;
    for i in 0..connector_count {
        let name = format!("exec-conn-{}", i);
        let name: &'static str = Box::leak(name.into_boxed_str());
        let connector = Box::new(PerfTestConnector { name });
        registry.register(name, connector).await.unwrap();
    }

    // 并发执行测试
    let total_executions = 10000;
    let concurrency = 200;
    let executions_per_task = total_executions / concurrency;

    let start = Instant::now();
    let mut handles = vec![];

    for task_id in 0..concurrency {
        let registry = Arc::clone(&registry);
        let handle = tokio::spawn(async move {
            let mut success_count = 0;
            for i in 0..executions_per_task {
                let conn_idx = (task_id * executions_per_task + i) % connector_count;
                let name = format!("exec-conn-{}", conn_idx);

                if let Ok(connector) = registry.get(&name).await {
                    let params = ValidatedParams::new(json!({"index": i}));
                    if connector.execute(params).await.is_ok() {
                        success_count += 1;
                    }
                }
            }
            success_count
        });
        handles.push(handle);
    }

    let total_success: usize = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .sum();

    let elapsed = start.elapsed();
    let ops_per_sec = total_executions as f64 / elapsed.as_secs_f64();

    println!("执行 {} 次操作耗时: {:?}", total_executions, elapsed);
    println!("吞吐量: {:.2} 操作/秒", ops_per_sec);
    println!(
        "成功率: {:.2}%",
        (total_success as f64 / total_executions as f64) * 100.0
    );

    assert_eq!(total_success, total_executions);
}

// ============================================================================
// 5. 压力测试
// ============================================================================

#[tokio::test]
async fn test_high_contention_scenario() {
    println!("\n=== 高竞争场景测试 ===");

    let registry = Arc::new(ConnectorRegistry::new());
    let duration = Duration::from_secs(5);

    // 启动多个并发任务进行混合操作
    let mut handles = vec![];

    // 写操作任务
    for writer_id in 0..5 {
        let registry = Arc::clone(&registry);
        let handle = tokio::spawn(async move {
            let mut count = 0;
            let start = Instant::now();

            while start.elapsed() < duration {
                let name = format!("writer{}-item{}", writer_id, count);
                let name: &'static str = Box::leak(name.into_boxed_str());
                let _ = registry
                    .register(name, Box::new(PerfTestConnector { name }))
                    .await;
                count += 1;

                // 偶尔删除
                if count % 10 == 0 && count > 10 {
                    let old_name = format!("writer{}-item{}", writer_id, count - 10);
                    let _ = registry.unregister(&old_name).await;
                }
            }
            count
        });
        handles.push(handle);
    }

    // 读操作任务
    for reader_id in 0..10 {
        let registry = Arc::clone(&registry);
        let handle = tokio::spawn(async move {
            let mut count = 0;
            let start = Instant::now();

            while start.elapsed() < duration {
                let _ = registry.list_all().await;
                count += 1;

                // 偶尔获取特定连接器
                if count % 5 == 0 {
                    let name = format!("writer{}-item{}", reader_id % 5, count / 2);
                    let _ = registry.get(&name).await;
                }
            }
            count
        });
        handles.push(handle);
    }

    let results: Vec<usize> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    let total_ops: usize = results.iter().sum();

    println!("5秒压力测试完成");
    println!("总操作数: {}", total_ops);
    println!("吞吐量: {:.2} 操作/秒", total_ops as f64 / 5.0);
    println!("写操作任务统计: {:?}", &results[0..5]);
    println!("读操作任务统计: {:?}", &results[5..15]);

    // 验证注册表仍然一致
    let final_count = registry.len().await;
    println!("最终注册表大小: {}", final_count);

    // 清理
    registry.clear().await;
}

// ============================================================================
// 6. 超时配置性能测试
// ============================================================================

#[tokio::test]
async fn test_timeout_config_validation_performance() {
    println!("\n=== 超时配置验证性能测试 ===");

    let config = TimeoutConfig::new()
        .with_min_timeout(100)
        .with_max_timeout(10000)
        .with_default_timeout(5000);

    let iterations = 100000;

    let start = Instant::now();
    for i in 0..iterations {
        let timeout = Duration::from_millis(100 + (i % 9000) as u64);
        let _ = config.effective_timeout(Some(timeout));
    }
    let elapsed = start.elapsed();

    println!("验证 {} 次超时配置耗时: {:?}", iterations, elapsed);
    println!(
        "每次验证耗时: {:.3} μs",
        elapsed.as_micros() as f64 / iterations as f64
    );
}

// ============================================================================
// 7. ValidatedParams 性能测试
// ============================================================================

#[tokio::test]
async fn test_validated_params_performance() {
    println!("\n=== ValidatedParams 性能测试 ===");

    let iterations = 100000;

    // 创建大 JSON 对象
    let large_json = json!({
        "data": (0..100).map(|i| json!({"id": i, "value": format!("value-{}", i)})).collect::<Vec<_>>()
    });

    // 测试创建性能
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = ValidatedParams::new(large_json.clone());
    }
    let creation_time = start.elapsed();

    println!(
        "创建 {} 次 ValidatedParams (大数据) 耗时: {:?}",
        iterations, creation_time
    );
    println!(
        "每次创建: {:.3} μs",
        creation_time.as_micros() as f64 / iterations as f64
    );

    // 测试元数据操作性能
    let params = ValidatedParams::new(json!({"key": "value"}));
    let start = Instant::now();
    for i in 0..iterations {
        let mut p = params.clone();
        p.set_metadata(format!("key{}", i), json!(i));
    }
    let metadata_time = start.elapsed();

    println!("元数据操作 {} 次耗时: {:?}", iterations, metadata_time);
    println!(
        "每次操作: {:.3} μs",
        metadata_time.as_micros() as f64 / iterations as f64
    );
}
