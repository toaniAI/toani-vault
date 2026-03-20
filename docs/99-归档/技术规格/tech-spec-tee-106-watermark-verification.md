---
title: 'TEE-106 - 水印验证逻辑实现'
slug: 'tee-106-watermark-verification'
created: '2026-03-19'
status: 'implemented'
priority: 'P1'
---

# Tech-Spec: TEE-106 - 水印验证逻辑实现

## 概述

### 问题陈述

`WatermarkService::verify_watermark` 方法当前是一个空实现，始终返回 `Ok(true)`，完全没有进行任何验证。这意味着任何伪造或缺失水印的截图都会被系统接受，违背了截图安全水印机制的根本目的——验证截图确实来自可信的 Enclave 执行环境。

当前代码（`src/tee/sandbox/export/watermark.rs:145-149`）：

```rust
/// 验证水印
pub fn verify_watermark(&self, _image_data: &[u8]) -> Result<bool, ExportError> {
    // TODO(#TEE-106): 实现水印验证逻辑
    // 需要: 提取水印信息并验证完整性
    Ok(true)
}
```

### 解决方案

实现完整的水印验证逻辑。由于当前水印采用像素点阵方式嵌入（非隐写），验证策略应分为两层：

1. **元数据验签层**：水印嵌入时，同时对水印文本内容生成 HMAC-SHA256 签名，附加到图片元数据（PNG tEXt chunk）中。验证时重新提取并校验签名。
2. **降级视觉验证层**：如果图片元数据不可用（如图片被重新编码），通过扫描图片中已知颜色的像素模式（水印背景矩形区域的半透明黑色块）进行存在性检测，作为次级检查。

核心改造点：
- `add_watermark` 流程中增加对水印文本的 HMAC 签名，写入 PNG tEXt 元数据。
- `verify_watermark` 中读取 PNG tEXt 元数据，提取签名并使用相同密钥验证。
- `WatermarkService` 增加 `hmac_key` 字段，从 `EnclaveKeyManager` 获取。

### 范围

**在范围内：**
- 实现 `verify_watermark` 方法的真实验证逻辑
- 修改 `add_watermark` 流程，在生成水印时将签名嵌入 PNG 元数据
- 为 `WatermarkService` 添加 HMAC 密钥支持
- 为新逻辑编写单元测试

**不在范围内：**
- 修改 `WatermarkConfig` 或 `WatermarkMetadata` 结构体的公开接口
- 实现对 JPEG/WebP 格式的元数据嵌入（当前仅处理 PNG）
- 隐写术（LSB steganography）等复杂方案

---

## 开发上下文

### 当前代码（关键片段）

**需修改的目标函数** — `src/tee/sandbox/export/watermark.rs:145`

```rust
/// 验证水印
///
/// 检查图片是否包含有效的水印信息
pub fn verify_watermark(&self, _image_data: &[u8]) -> Result<bool, ExportError> {
    // TODO(#TEE-106): 实现水印验证逻辑
    // 需要: 提取水印信息并验证完整性
    Ok(true)
}
```

**水印添加入口** — `src/tee/sandbox/export/watermark.rs:97-120`

```rust
pub fn add_watermark(
    &self,
    image_data: &[u8],
    metadata: &WatermarkMetadata,
    config: &WatermarkConfig,
) -> Result<Vec<u8>, ExportError> {
    info!("开始为会话 {} 的截图添加水印", metadata.session_id);
    let watermark_text = self.build_watermark_text(metadata, config);
    debug!("水印内容: {}", watermark_text);
    match self.apply_watermark_with_image(image_data, &watermark_text, config) {
        Ok(data) => {
            info!("水印添加完成");
            Ok(data)
        }
        Err(e) => {
            warn!("使用 image crate 添加水印失败: {}, 使用回退实现", e);
            self.apply_watermark_mock(image_data, &watermark_text, config)
        }
    }
}
```

**PNG 编码函数**（签名将在此处之后写入元数据）— `src/tee/sandbox/export/watermark.rs:388-397`

```rust
fn encode_image(&self, image: &DynamicImage) -> Result<Vec<u8>, ExportError> {
    let mut output = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut output);
    image
        .write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| ExportError::Watermark(format!("编码图片失败: {}", e)))?;
    Ok(output)
}
```

**WatermarkService 结构体**（需增加密钥字段）— `src/tee/sandbox/export/watermark.rs:13-15`

```rust
pub struct WatermarkService {
    enclave_id: String,
}
```

### 相关代码结构

- `build_watermark_text` 方法（行 152-182）：将元数据拼接为 `"Enclave: xxx | Session: yyy | Time: zzz"` 格式的字符串，此字符串即为需要被签名的内容。
- `apply_watermark_with_image` 方法（行 185-232）：处理 PNG 解码、绘制、编码的完整流程，是插入签名逻辑的合适位置。
- `apply_watermark_mock` 方法（行 400-409）：回退实现，直接返回原始数据，无法嵌入签名，`verify_watermark` 对其产物应返回 `false`。
- `ExportError` 枚举：位于 `src/tee/sandbox/error.rs`，包含 `ExportError::Watermark(String)` 变体，可复用于新错误。

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/tee/sandbox/export/watermark.rs` | 主要修改目标文件 |
| `src/tee/sandbox/error.rs` | ExportError 定义，用于错误处理 |
| `src/crypto/enclave_key.rs` | EnclaveKeyManager，用于获取签名密钥材料 |
| `Cargo.toml` | 确认 `hmac`、`sha2`、`png` crate 是否已引入 |

### 技术决策

1. **签名算法**：使用 HMAC-SHA256。密钥从 `WatermarkService` 构造时传入（`&[u8]`），调用方从 `EnclaveKeyManager` 获取，避免 `WatermarkService` 直接依赖密钥管理器。
2. **嵌入位置**：PNG tEXt chunk，key 为 `"CredBridge-Watermark-Sig"`，value 为 `base64(HMAC-SHA256(watermark_text, key))`。使用 `png` crate 直接操作 chunk，避免重走 `image` crate 的 encode 流程。
3. **验签内容**：验证 `watermark_text` 的完整性（即元数据字段的拼接字符串），而不是整个图片数据。这样即使图片被无损重新保存，签名仍然有效。
4. **兼容性**：若 PNG 中不存在签名 chunk（旧版截图或非 PNG 格式），`verify_watermark` 返回 `Ok(false)` 而非 `Err`。
5. **密钥初始化**：`WatermarkService::new` 签名不变（仍接收 `enclave_id`），新增 `WatermarkService::with_key(enclave_id, hmac_key: Vec<u8>)` 构造方法，`default_service` 使用一个固定的开发占位密钥，生产环境必须使用 `with_key`。

---

## 实现计划

### 任务（按依赖顺序）

1. **更新 `WatermarkService` 结构体，添加 HMAC 密钥字段** — `src/tee/sandbox/export/watermark.rs:13`
   - 在 `WatermarkService` 中增加 `hmac_key: Vec<u8>` 字段
   - 添加 `with_key(enclave_id: impl Into<String>, hmac_key: Vec<u8>) -> Self` 构造方法
   - `default_service()` 使用固定占位密钥 `b"credbridge-dev-watermark-key-000"` (32字节)
   - 更新所有已有的构造调用点（检查是否有其他文件通过 `WatermarkService::new` 构造）

2. **实现 PNG tEXt chunk 写入辅助函数** — `src/tee/sandbox/export/watermark.rs`（在 `encode_image` 之后新增私有方法）
   - 新增私有方法 `fn embed_signature_in_png(png_data: &[u8], watermark_text: &str, hmac_key: &[u8]) -> Result<Vec<u8>, ExportError>`
   - 使用 `hmac` + `sha2` crate 计算 `HMAC-SHA256(hmac_key, watermark_text.as_bytes())`
   - 将签名 base64 编码后，通过 `png` crate 追加 tEXt chunk（key: `"CredBridge-Watermark-Sig"`）到 PNG 数据末尾（IEND chunk 之前）
   - 若 PNG 解析失败，返回 `ExportError::Watermark("无法嵌入签名: ...".into())`

3. **修改 `apply_watermark_with_image`，在编码后调用签名嵌入** — `src/tee/sandbox/export/watermark.rs:185`
   - 在 `self.encode_image(&image)?` 返回的 `Vec<u8>` 上，调用步骤 2 的函数
   - 将 `watermark_text` 通过参数传递到调用链（当前已作为参数传入，无需修改签名）
   - 失败时 `warn!` 并返回未嵌入签名的原始 PNG 数据（降级处理，不中断截图流程）

4. **实现 `verify_watermark`** — `src/tee/sandbox/export/watermark.rs:145`
   - 解析入参 `image_data` 为 PNG，从 tEXt chunks 中查找 key 为 `"CredBridge-Watermark-Sig"` 的 chunk
   - 若不存在，返回 `Ok(false)`
   - 将 chunk value base64 解码，得到存储的 HMAC 签名
   - **注意**：此处验证存在"需要知道原始 `watermark_text` 才能重新计算 HMAC" 的问题。解决方案：同时在另一个 tEXt chunk（key: `"CredBridge-Watermark-Text"`）中存储原始水印文本（明文，不涉及机密）。验证时先读取 `Text` chunk，再用其内容重新计算 HMAC，与 `Sig` chunk 对比。
   - 若两者匹配，返回 `Ok(true)`；若 base64 解码失败或 HMAC 不匹配，返回 `Ok(false)`；若 PNG 解析错误，返回 `Err(ExportError::Watermark(...))`

5. **确认 Cargo.toml 已包含所需依赖** — `Cargo.toml`
   - 检查是否已有 `hmac`、`sha2`、`png`（或 `image` 已包含 png 支持）、`base64` crate
   - 若缺少，添加：`hmac = "0.12"`、`sha2 = "0.10"`（`base64` 已通过 websocket.rs 引入）
   - `png` crate 操作 chunk 可能需要 `png = "0.17"`

6. **编写单元测试** — `src/tee/sandbox/export/watermark.rs`（`#[cfg(test)]` 模块）
   - 补充 `test_verify_watermark_valid`：对同一图片先调用 `add_watermark` 再调用 `verify_watermark`，断言返回 `Ok(true)`
   - 补充 `test_verify_watermark_no_signature`：对未加水印的 PNG 调用 `verify_watermark`，断言返回 `Ok(false)`
   - 补充 `test_verify_watermark_wrong_key`：用 key-A 签名、用 key-B 验证，断言返回 `Ok(false)`
   - 补充 `test_verify_watermark_tampered`：修改 `Sig` chunk 内容后验证，断言返回 `Ok(false)`

### 验收标准

- **Given** 一张通过 `add_watermark` 生成的 PNG 截图，
  **When** 使用相同密钥调用 `verify_watermark`，
  **Then** 返回 `Ok(true)`。

- **Given** 一张普通 PNG（无水印签名），
  **When** 调用 `verify_watermark`，
  **Then** 返回 `Ok(false)`（不是 `Err`，不是 `Ok(true)`）。

- **Given** 一张水印 PNG 的 `CredBridge-Watermark-Sig` chunk 被篡改，
  **When** 调用 `verify_watermark`，
  **Then** 返回 `Ok(false)`。

- **Given** 使用密钥 A 生成的水印 PNG，
  **When** 使用密钥 B 构造的 `WatermarkService` 调用 `verify_watermark`，
  **Then** 返回 `Ok(false)`。

- **Given** `apply_watermark_mock`（回退路径）生成的图片，
  **When** 调用 `verify_watermark`，
  **Then** 返回 `Ok(false)`（因为回退实现不嵌入签名）。

- `WatermarkService::default_service()` 原有构造方式兼容，现有调用不需要修改。

---

## 附加上下文

### 依赖

- `hmac = "0.12"` + `sha2 = "0.10"`：计算 HMAC-SHA256
- `png = "0.17"`（可选，若 `image` crate 不暴露 chunk 操作 API 则需要）：读写 PNG tEXt chunk
- `base64`：已通过 `use base64::{Engine as _, engine::general_purpose::STANDARD}` 引入（见 websocket.rs:34）

### 测试策略

- 所有测试均为纯单元测试，不依赖外部进程或文件系统
- 使用 `image::ImageBuffer` 创建内存中的测试 PNG 作为输入
- 测试密钥使用固定字节数组，避免随机性干扰

### 注意事项

- PNG tEXt chunk 只适用于 PNG 格式。若 `apply_watermark_mock` 回退路径被触发（非 PNG 输入或 image crate 解码失败），原始数据原样返回，无法嵌入签名。`verify_watermark` 对此类输入应优雅处理（返回 `Ok(false)`），而不是 panic。
- HMAC 密钥是验证链的信任根，`default_service` 的占位密钥**只能用于开发测试环境**，生产部署必须通过 `with_key` 注入来自 `EnclaveKeyManager` 的密钥材料。
- 本实现不提供针对图片整体内容的完整性保护（不签名像素数据），仅保证水印元数据字段的完整性。如果需要图片像素级完整性保护，应在 `ScreenshotService` 层通过 `EnclaveKeyManager::sign` 签名整个截图字节流（这是另一个独立任务的范围）。
