# API 层代码审查报告 — 对抗性审查

**审查日期**: 2026-03-19
**审查范围**: `src/api/audit.rs`, `src/api/credentials.rs`, `src/api/websocket.rs`, `tests/api/audit_tests.rs`
**参考 Tech-Spec**: tech-spec-audit-security-context.md, tech-spec-audit-signature-verification.md
**审查立场**: 对抗性（Adversarial）— 假设存在安全问题，主动寻找漏洞

---

## 总体评级

| 模块 | 安全评级 | 实现质量 | 测试覆盖 |
|------|---------|---------|---------|
| `audit.rs` | B（有改进但存在遗留问题） | B | C+ |
| `credentials.rs` | C+（mrenclave 硬编码未完全消除） | B+ | C |
| `websocket.rs` | D（核心功能未实现，安全漏洞） | D | D |
| `audit_tests.rs` | — | — | C（覆盖面浅）|

---

## 1. Audit API (`src/api/audit.rs`)

### 1.1 Tech-Spec 符合性检查

#### 问题 2（content_hash_match）— 已修复 ✓

Tech-Spec `tech-spec-audit-security-context.md` 指出 `content_hash_match` 原固定为 `true`。当前代码：

```rust
// audit.rs:657-663
let computed_content_hash = entry.entry.content_hash();
let content_hash_match = computed_content_hash == entry.content_hash;
details.push(VerificationDetail {
    step: "内容哈希验证".to_string(),
    passed: content_hash_match,
    message: Some(format!("哈希: {}", hex::encode(entry.content_hash))),
});
```

**修复状态**: 已正确实现真实计算。内容哈希现在通过 `entry.entry.content_hash()` 重新计算并与存储值比较。

**遗留问题**: Tech-Spec 要求 `VerificationDetail.message` 同时显示存储哈希和计算哈希（"存储哈希: ..., 计算哈希: ..."），但当前代码只显示存储哈希 `hex::encode(entry.content_hash)`，不显示计算哈希。当哈希不匹配时，无法从响应中直接看出计算值，给运维调试增加障碍。

- **文件:行号**: `audit.rs:662`
- **严重程度**: Low（功能正确，仅为调试信息缺失）

#### P1 问题（签名验证）— 已修复 ✓

Tech-Spec `tech-spec-audit-signature-verification.md` 指出原签名验证为 `!entry.signature.is_empty()`（假验证）。当前代码：

```rust
// audit.rs:666-671
let signature_valid = {
    let combined_data = [entry.content_hash.as_slice(), entry.prev_hash.as_slice()].concat();
    let combined_hash = digest(&SHA256, &combined_data);
    let pub_key = UnparsedPublicKey::new(&ED25519, &state.verifier_public_key);
    pub_key.verify(combined_hash.as_ref(), &entry.signature).is_ok()
};
```

**修复状态**: 已正确实现 Ed25519 真实签名验证，签名数据格式 `SHA256(content_hash || prev_hash)` 与 `AuditRecorder::record` 保持一致。

### 1.2 安全问题

#### [SECURITY-HIGH] verifier_public_key 为空时静默失败

当 `verifier_public_key` 为空 `Vec<u8>` 时，`ring::signature::UnparsedPublicKey::new(&ED25519, &[])` 会在 `.verify()` 时返回 `Err`，导致 `signature_valid = false`。这是 fail-closed 行为，安全性正确。

**但存在问题**：没有任何日志或告警记录"公钥为空导致验证失败"这一事件。运维人员无法区分"公钥配置缺失"与"数据被真实篡改"两种情况，给故障排查带来盲区。

- **文件:行号**: `audit.rs:669-670`
- **严重程度**: Medium

#### [SECURITY-MEDIUM] 导出接口无速率限制、无导出量上限

`export_audit_logs`（L404-526）调用 `state.storage.get_all(filter).await`，在 `MemoryAuditStorageAdapter::get_all` 中最终调用 `self.query(filter, 0, 100_000).await`。即使过滤器全为空，单次请求可导出最多 10 万条记录，响应体可能达到数十 MB。

对于凭证管理系统，大批量导出审计日志属于高敏感操作，应有：
1. 导出记录数上限（当前无限）
2. 速率限制（当前无限）
3. 导出事件本身应产生审计日志（当前未记录"谁在何时导出了日志"）

- **文件:行号**: `audit.rs:474`, `audit.rs:113-124`（MemoryAuditStorageAdapter::query 硬编码 `100_000`）
- **严重程度**: High

#### [SECURITY-MEDIUM] CSV 导出无字段转义，存在 CSV 注入风险

`export_to_csv`（L529-556）直接将 `entry.entry.service`、`entry.entry.user_id_hash` 等字段拼入 CSV，未做任何转义。如果字段值中包含 `,`、`\n`、`"` 或以 `=`、`+`、`-`、`@` 开头（CSV 注入前缀），在 Excel/LibreOffice 等电子表格软件打开时可能执行任意公式或破坏 CSV 结构。

- **文件:行号**: `audit.rs:537-551`
- **严重程度**: Medium（取决于导出文件是否由 office 软件处理）

#### [SECURITY-LOW] 导出内容用 Base64 编码但 integrity_hash 基于编码前内容

`integrity_hash` 计算于 `content.as_bytes()`（L507），但 `content` 随即被 Base64 编码存入响应（L513）。接收方若要校验完整性，需先 Base64 解码再计算 SHA256，文档中无此说明，给使用方造成歧义。

- **文件:行号**: `audit.rs:507-513`
- **严重程度**: Low

### 1.3 实现质量问题

#### [BUG] `export_audit_logs` 过滤器丢失 `risk_tier`、`outcome`、`service` 字段

构建导出过滤器时（L463-471）：

```rust
let filter = AuditFilter {
    start_time: params.start_time,
    end_time: params.end_time,
    user_id_hash: params.user_id_hash.clone(),
    action: params.action,
    risk_tier: None,    // 硬编码 None，忽略请求参数
    outcome: None,      // 硬编码 None
    service: None,      // 硬编码 None
};
```

即使 `AuditExportRequest` 中包含这些字段（需确认），导出时也无法按 `risk_tier`/`outcome`/`service` 过滤，与查询接口行为不一致。

- **文件:行号**: `audit.rs:468-470`
- **严重程度**: Medium（功能缺失）

#### [BUG] `MemoryAuditStorageAdapter::verify_entry` 验证的是整链而非单条

```rust
// audit.rs:199-211
async fn verify_entry(&self, index: u64) -> Result<bool, String> {
    let storage = self.storage.lock().await;
    let entry = storage.get_by_index(index).map_err(|e| e.to_string())?;
    if entry.is_none() {
        return Ok(false);
    }
    storage.verify().map_err(|e| e.to_string())  // 验证的是整个链
}
```

调用 `storage.verify()` 是对完整链的验证，而不是针对 `index` 指定条目的验证。对于一个有 10 万条记录的链，每次单条验证请求都会触发全链遍历，性能极差（O(n)）。同时，如果整链中有任意一条记录损坏，对任意索引的验证都会返回 `false`，使得 `merkle_proof_valid` 字段语义不准确。

- **文件:行号**: `audit.rs:199-211`
- **严重程度**: Medium（性能 + 语义问题）

#### [QUALITY] `usize::MAX` 作为 body 读取上限，存在 OOM 风险

```rust
// audit.rs:410, 567
let bytes = match axum::body::to_bytes(body, usize::MAX).await {
```

`usize::MAX` 在 64 位系统上为 18EB，相当于无上限读取。恶意客户端可发送超大请求体导致服务器 OOM。应限制为合理的最大值（如 64KB 或 1MB）。

- **文件:行号**: `audit.rs:410`, `audit.rs:567`
- **严重程度**: High（DoS 攻击向量）

#### [QUALITY] `current_timestamp_millis` 使用 `.expect()`，违反生产代码规范

```rust
// audit.rs:728-730
SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .expect("系统时间错误")  // 生产代码禁止 expect()
    .as_millis() as u64
```

Rust Coding Standards 明确禁止生产代码中使用 `expect()`。虽然系统时间倒退的概率极低，但仍应使用 `unwrap_or_default()` 或返回 `Result`。

- **文件:行号**: `audit.rs:730`
- **严重程度**: Low

---

## 2. Credentials API (`src/api/credentials.rs`)

### 2.1 Tech-Spec 符合性检查

#### 问题 1（mrenclave/jti 真实值）— 部分修复 ⚠

Tech-Spec 要求从请求上下文获取真实的 `jti`，从 TEE 状态获取真实的 `mrenclave`。

**`jti` 修复状态**: 已修复。`jti` 现在从 `&token.token_id` 获取（L399, L533, L595-596, L710, 等），而非硬编码 `"jti"` 字符串。

**`mrenclave` 修复状态**: 未完全修复。所有调用处仍使用硬编码字符串 `"software_mode"`：

```rust
// credentials.rs:401, 533, 596, 606, 715
state.audit_logger.log_credential_created(
    &token.tenant_id,
    &token.user_id,
    entry.credential_id.as_str(),
    &token.token_id,
    "software_mode",  // ← 硬编码，未从 TEE 获取
);
```

Tech-Spec 要求：`mrenclave` 从 `crate::tee::get_mrenclave()` 或等效函数获取；若 TEE 不可用，使用 `"simulation"` 并记录 warn。当前方案：
- 使用 `"software_mode"` 而非 Tech-Spec 建议的 `"simulation"` 标识符（字符串不一致）
- 未调用 TEE 模块的任何接口
- 未记录 warn

验收标准"TEE 不可用时优雅降级（记录 warn，使用明确的占位符而非误导性的 `"mrenclave"` 字符串）"仅部分满足（有明确占位符，但无 warn 日志）。

- **文件:行号**: `credentials.rs:401`, `533`, `596`, `606`, `715`
- **严重程度**: Medium（安全审计真实性受损）

### 2.2 安全问题

#### [SECURITY-HIGH] 解密失败时审计日志仍可能泄露凭证 ID

`decrypt_credential_endpoint`（L564-633）在解密失败时记录审计日志（L601-610），传入 `&id`（从路径参数直接获取的字符串），此时 `id` 未经验证是否属于当前租户。结合下面的所有权检查问题一起看：

```rust
// credentials.rs:581-585
let entry = state
    .vault
    .get_credential(&credential_id, &tenant_id, &user_id)
    .map_err(|e| ApiError::new("internal_error", e.to_string()))?
    .ok_or_else(|| ApiError::new("not_found", "凭证不存在"))?;
```

`get_credential` 传入了 `tenant_id` 和 `user_id`，如果 vault 实现正确地做了租户隔离，则不存在跨租户访问。但审计日志中的 `credential_id` 来自路径参数 `&id`（L593），而非 `entry.credential_id.as_str()`，这意味着即使凭证不存在，失败审计日志也会记录攻击者探测的任意 ID。这是可接受行为，但审计系统需要能区分"真实失败"与"探测行为"。

- **文件:行号**: `credentials.rs:593`, `602`
- **严重程度**: Low（依赖 vault 隔离正确性）

#### [SECURITY-MEDIUM] `record_to_storage` 使用 `try_lock` 导致审计日志可能丢失

```rust
// credentials.rs:163-164
if let Ok(storage) = self.storage.try_lock() {
    // 如果锁不可用则跳过存储
```

`try_lock` 在锁被占用时会跳过审计日志写入，并只记录一条 warn。对于凭证解密这类高敏感操作，丢失审计日志是不可接受的。应使用 `lock().await` 确保日志写入，或使用无锁的 channel 方式异步写入。

- **文件:行号**: `credentials.rs:163-164`
- **严重程度**: High（高敏感操作审计日志可丢失）

#### [SECURITY-MEDIUM] `with_param` 参数命名错误，`tenant_id` 字段存入 `user_id` 值

```rust
// credentials.rs:177-181
.with_param(
    "credential_id",
    RedactedParam::Plain(credential_id.to_string()),
)
.with_param("tenant_id", RedactedParam::Plain(user_id.to_string())); // ← BUG
```

参数键名为 `"tenant_id"`，但传入的值是 `user_id`（函数参数名 `user_id: &str`）。在调用处，传入的 `user_id` 是用户 ID 字符串，不是租户 ID。这导致审计日志中 `tenant_id` 字段实际记录的是用户 ID，审计数据污染。

- **文件:行号**: `credentials.rs:180-181`
- **严重程度**: Medium（审计数据正确性问题）

#### [SECURITY-LOW] `session_id` 固定为字面量 "session"

```rust
// credentials.rs:170
"session",     // session_id
```

所有凭证操作的审计日志 `session_id` 均为 `"session"` 字符串，无法追踪同一会话内的操作序列。Tech-Spec 对此未明确要求，但对于会话追踪是一个明显的缺陷。

- **文件:行号**: `credentials.rs:170`
- **严重程度**: Low

### 2.3 密钥派生与加密逻辑

#### [QUALITY] `encrypt_credential_in_tee` 使用 `write` lock 执行只读操作

```rust
// credentials.rs:433
let hierarchy = state.key_hierarchy.write().await;
```

密钥派生是只读操作（`derive_user_vault_key`、`derive_credential_key` 应为纯函数），但获取的是 `write` lock，不必要地阻塞其他读操作，降低并发性能。`decrypt_credential_in_tee`（L652）同样存在此问题。

- **文件:行号**: `credentials.rs:433`, `653`
- **严重程度**: Low（性能问题）

#### [QUALITY] `DefaultAuditLogger` 未被使用的潜在安全风险

`DefaultAuditLogger`（L80-135）仅将审计事件打印到控制台（`log::info!`），不写入任何持久存储。如果系统错误地使用了 `DefaultAuditLogger` 而非 `StorageAuditLogger`，所有审计日志都会静默丢失（仅有控制台输出）。代码中无任何机制防止生产环境误用。

- **文件:行号**: `credentials.rs:80-135`
- **严重程度**: Medium（错误配置风险）

### 2.4 错误处理

#### [QUALITY] 解密端点明文错误信息泄露内部细节

```rust
// credentials.rs:609
return Err(ApiError::new("internal_error", e));
```

`e` 是解密失败的内部错误字符串（如 "解密失败: tag mismatch"），直接暴露给客户端，可能泄露加密方案细节。应使用通用错误消息，内部错误记录到日志。

- **文件:行号**: `credentials.rs:609`
- **严重程度**: Medium

---

## 3. WebSocket API (`src/api/websocket.rs`)

### 3.1 核心功能问题

#### [CRITICAL] 核心操作功能未实现，但未做明确的降级保护

`execute_operation`（L625-647）明确返回错误：

```rust
Err(format!(
    "沙箱会话池尚未集成 (session={}, op={:?}): 请通过 SandboxSessionPool 建立会话后重试",
    state.session_id, operation.operation_type
).into())
```

这导致所有 `Execute` 类型的 WebSocket 消息都会返回失败。问题在于：
1. 此端点对外暴露（通过 `sandbox_websocket_handler`），客户端可以连接并发送操作，但永远得不到成功结果
2. 没有在握手阶段（`Connected` 消息）告知客户端当前服务处于降级状态
3. 路由注册时没有任何保护措施（如功能标志）防止客户端意外使用未完成的功能

- **文件:行号**: `websocket.rs:625-647`
- **严重程度**: Critical（功能完整性）

#### [SECURITY-HIGH] WebSocket 端点缺少权限验证

`sandbox_websocket_handler`（L219-231）的函数签名：

```rust
pub async fn sandbox_websocket_handler(
    Path((session_id, credential_id)): Path<(String, String)>,
    State(ctx): State<ApiContext>,
    ws: WebSocketUpgrade,
    token: ValidatedToken,
) -> Response {
```

`token: ValidatedToken` 作为 Axum Extractor 提取（而非 `Extension<ValidatedToken>`），但无法确认中间件是否正确注入了已验证的 token。更严重的问题是：**进入 handler 后没有任何 scope 检查**。任何持有有效 token（包括只有 `credential:read` scope）的用户都可以连接沙箱 WebSocket 并尝试执行操作，没有检查用户是否有沙箱执行权限。

- **文件:行号**: `websocket.rs:219-231`
- **严重程度**: High（权限绕过）

#### [SECURITY-HIGH] session_id 与 credential_id 未做所有权验证

`handle_socket`（L234-413）解析了 `session_id` 和 `credential_id`，初始化了 `ConnectionState`，但从未验证：
1. 当前用户是否拥有该 `session_id` 对应的会话
2. 当前用户是否有权访问 `credential_id` 对应的凭证
3. `session_id` 与 `credential_id` 是否属于同一租户

```rust
// websocket.rs:288-299
let state = ConnectionState {
    session_id,
    tenant_id,
    user_id,
    credential_id,  // ← 未验证所有权
    ...
};
```

恶意用户可以猜测其他用户的 `session_id` 或 `credential_id` 并尝试连接。

- **文件:行号**: `websocket.rs:288-299`
- **严重程度**: High

#### [SECURITY-MEDIUM] 心跳机制虚设，连接超时未基于真实活跃时间

```rust
// websocket.rs:322-331
let heartbeat_handle = tokio::spawn(async move {
    let mut interval = interval(Duration::from_secs(heartbeat_interval));
    loop {
        interval.tick().await;
        // 心跳由客户端发起，服务端只响应
        // 这里可以添加超时检查逻辑
    }
});
```

心跳任务只是一个空循环，不做任何事。同时，连接超时通过 `tokio::time::sleep(Duration::from_secs(config.connection_timeout))` 实现（L396），这是**固定延时**而非基于最后活跃时间的滑动窗口。如果客户端一直发送消息，`select!` 会一直选择消息分支，超时分支永远不会触发（因为 sleep future 被重置）。实际上连接永远不会超时。

- **文件:行号**: `websocket.rs:322-331`, `396`
- **严重程度**: Medium（DoS：客户端可以保持连接永不超时）

#### [SECURITY-LOW] 操作参数（`parameters: HashMap<String, Value>`）无输入验证

`Execute` 消息中的 `parameters` 是任意 JSON 对象，无大小限制、无字段验证。恶意客户端可发送极大的 `parameters` 占用服务器内存。

- **文件:行号**: `websocket.rs:61`, `452-454`
- **严重程度**: Low

### 3.2 消息处理问题

#### [BUG] `handle_message` 中 `Close` 分支返回错误字符串，非优雅关闭

```rust
// websocket.rs:527-531
ClientMessage::Close { reason } => {
    info!("Client requested close: {:?}", reason);
    return Err("Client requested close".into());
}
```

客户端请求关闭时，通过返回 `Err` 来触发主循环的 `warn!("Error handling message: {}")` 日志。这会将正常的客户端关闭记录为警告级别的错误，导致日志噪音，影响监控告警的有效性。

- **文件:行号**: `websocket.rs:527-531`
- **严重程度**: Low

#### [BUG] WebSocket 消息通道发送使用 `let _ =`，静默丢弃发送失败

```rust
// websocket.rs:446, 489, 516
let _ = tx.send(progress_msg).await;
let _ = tx.send(completed_msg).await;
let _ = tx.send(result_msg).await;
```

通道发送失败（接收端已关闭）被静默忽略，不记录日志。若主循环已退出，进度/完成消息将丢失，客户端无法得知操作状态。

- **文件:行号**: `websocket.rs:446`, `489`, `516`
- **严重程度**: Low

### 3.3 实现质量

WebSocket 模块整体处于骨架阶段（核心功能未实现），测试仅覆盖消息序列化/反序列化，无任何安全场景测试。不应在生产环境启用此端点。

---

## 4. 测试质量 (`tests/api/audit_tests.rs`)

### 4.1 覆盖的路径

| 测试场景 | 覆盖 |
|---------|------|
| 成功列表查询 | ✓ |
| 分页参数 | ✓ |
| 过滤参数（action/outcome）| ✓ |
| 无权限返回 403 | ✓ |
| 无效分页（page=0）| ✓ |
| 按 ID 获取（404）| ✓（但断言弱，见下） |
| 按索引获取 | ✓ |
| 导出 JSON/CSV | ✓ |
| 时间范围过滤 | ✓ |
| 无效时间范围 | ✓ |
| 签名验证（有效/无效）| ✗ |
| 内容哈希篡改检测 | ✗ |
| 无 Token（401）| ✗ |
| 验证接口 403 | ✓ |

### 4.2 关键缺失的负面测试

#### [TEST-CRITICAL] 无签名验证相关测试

Tech-Spec `tech-spec-audit-signature-verification.md` 要求三个关键场景的测试：
1. 有效签名 → `signature_valid = true`
2. 篡改内容哈希 → `signature_valid = false`
3. 伪造非空签名（如 `vec![1,2,3]`）→ `signature_valid = false`

测试文件中**无任何一个**对签名验证行为的断言。`test_verify_audit_log_by_index` 仅断言 HTTP 状态码为 200，不检查 `signature_valid` 字段值。

测试构造的 `AuditApiState` 使用空公钥：
```rust
// audit_tests.rs:91-94
let state = AuditApiState {
    storage: std::sync::Arc::new(create_test_storage()),
    verifier_public_key: vec![],  // 空公钥
};
```

空公钥会导致签名验证永远返回 `false`，但测试未验证响应体中 `verified` 字段的值。这意味着即使签名验证逻辑被再次破坏（回到假验证），测试仍会通过。

- **文件:行号**: `audit_tests.rs:89-94`, `326-341`
- **严重程度**: Critical（测试对核心安全特性无保护能力）

#### [TEST-HIGH] 内容哈希篡改检测无测试

Tech-Spec 要求人为篡改 `AuditEntry` 后验证哈希不匹配。无对应测试用例。

#### [TEST-MEDIUM] `test_get_audit_log_detail_by_id` 断言过于宽松

```rust
// audit_tests.rs:192-193
assert!(response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND);
```

这个断言对任何响应状态码（200 或 404）都通过，实际上什么都没有验证。应先查询现有条目 ID，然后断言 200，分开测试真实存在 ID 和不存在 ID 的场景。

- **文件:行号**: `audit_tests.rs:192-193`
- **严重程度**: Medium

#### [TEST-LOW] 无对 401（未认证）场景的测试

测试文件测试了 403（权限不足），但没有测试完全缺少 Token 时应返回 401 的场景。

#### [TEST-LOW] `create_test_storage` 不通过真实签名器，签名字段为空

```rust
// audit_tests.rs:60-86
storage.record(entry).unwrap();  // MemoryAuditStorage::record 是否签名？
```

需要确认 `MemoryAuditStorage::record` 是否实际执行签名。如果不签名，则测试中所有 `SignedAuditEntry` 的签名字段为空/无效，测试验证路径与生产路径完全不同。

---

## 5. 综合问题汇总

### 高严重度问题（需优先修复）

| # | 文件 | 行号 | 问题 |
|---|------|------|------|
| 1 | `audit.rs` | L410, L567 | `usize::MAX` body 读取上限，DoS 风险 |
| 2 | `audit.rs` | L474 | 导出无量上限，可导出 10 万条 |
| 3 | `credentials.rs` | L163-164 | `try_lock` 导致高敏感操作审计日志可丢失 |
| 4 | `websocket.rs` | L219-231 | WebSocket 端点无 scope 权限检查 |
| 5 | `websocket.rs` | L288-299 | session/credential 未验证所有权 |
| 6 | `audit_tests.rs` | L89-94 | 签名验证核心安全特性无有效测试覆盖 |

### 中严重度问题

| # | 文件 | 行号 | 问题 |
|---|------|------|------|
| 7 | `audit.rs` | L537-551 | CSV 注入风险 |
| 8 | `audit.rs` | L199-211 | `verify_entry` 验证全链，O(n) 性能 |
| 9 | `audit.rs` | L668-670 | 公钥为空时无告警日志 |
| 10 | `credentials.rs` | L401等 | `mrenclave` 硬编码 "software_mode"，TEE 接口未调用 |
| 11 | `credentials.rs` | L180-181 | `tenant_id` 字段存入 `user_id` 值，审计数据污染 |
| 12 | `credentials.rs` | L80-135 | `DefaultAuditLogger` 错误配置风险 |
| 13 | `credentials.rs` | L609 | 解密失败内部错误直接暴露给客户端 |
| 14 | `websocket.rs` | L396 | 连接超时固定延时，不能真正限制长连接 |
| 15 | `audit_tests.rs` | L192-193 | 断言过于宽松，任何状态码都通过 |
| 16 | `audit.rs` | L468-470 | 导出过滤器丢失 risk_tier/outcome/service |

---

## 6. 对 Tech-Spec 实现的总体评估

| Tech-Spec 要求 | 实现状态 | 备注 |
|---------------|---------|------|
| P1: Ed25519 真实签名验证 | 已实现 ✓ | 逻辑正确，公钥注入机制待完善 |
| P2: content_hash 真实计算 | 已实现 ✓ | 逻辑正确，但 message 仅显示存储哈希 |
| P2: JTI 真实值（来自 token） | 已实现 ✓ | 所有调用处使用 `token.token_id` |
| P2: MRENCLAVE 真实值 | 部分实现 ⚠ | 使用 "software_mode" 占位符，但未调用 TEE 接口，无 warn 日志 |
| P2: 篡改检测测试用例 | 未实现 ✗ | 测试文件缺少所有安全相关断言 |

Tech-Spec 中的核心安全修复（签名验证、内容哈希）在代码层面已正确实现，但测试层面的保护缺失（`audit_tests.rs` 中无对应断言），意味着未来的回归风险高。
