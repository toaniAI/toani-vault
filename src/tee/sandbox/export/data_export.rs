//! 数据导出服务
//!
//! 提供结构化的安全数据导出，支持 JSON、CSV、PDF 等格式，
//! 集成签名验证和敏感数据脱敏。

use crate::crypto::enclave_key::{EnclaveKeyManager, Signature};
use crate::tee::sandbox::{
    error::ExportError, export::redaction::RedactionService, types::SessionId,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use time::OffsetDateTime;
use tracing::{debug, info, warn};

/// 导出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// JSON 格式
    Json,
    /// CSV 格式
    Csv,
    /// PDF 格式（可选）
    Pdf,
    /// XML 格式
    Xml,
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportFormat::Json => write!(f, "json"),
            ExportFormat::Csv => write!(f, "csv"),
            ExportFormat::Pdf => write!(f, "pdf"),
            ExportFormat::Xml => write!(f, "xml"),
        }
    }
}

impl ExportFormat {
    /// 获取 MIME 类型
    pub fn mime_type(&self) -> &'static str {
        match self {
            ExportFormat::Json => "application/json",
            ExportFormat::Csv => "text/csv",
            ExportFormat::Pdf => "application/pdf",
            ExportFormat::Xml => "application/xml",
        }
    }

    /// 获取文件扩展名
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::Csv => "csv",
            ExportFormat::Pdf => "pdf",
            ExportFormat::Xml => "xml",
        }
    }
}

/// 导出请求
#[derive(Debug, Clone)]
pub struct ExportRequest {
    /// 导出格式
    pub format: ExportFormat,
    /// 导出数据（JSON Value）
    pub data: serde_json::Value,
    /// 数据模式定义
    pub schema: ExportSchema,
    /// 是否脱敏敏感字段
    pub redact_sensitive: bool,
    /// 包含元数据
    pub include_metadata: bool,
    /// 自定义选项
    pub options: HashMap<String, serde_json::Value>,
}

impl ExportRequest {
    /// 创建新的导出请求
    pub fn new(format: ExportFormat, data: serde_json::Value) -> Self {
        Self {
            format,
            data,
            schema: ExportSchema::default(),
            redact_sensitive: true,
            include_metadata: true,
            options: HashMap::new(),
        }
    }

    /// 设置脱敏选项
    pub fn with_redaction(mut self, redact: bool) -> Self {
        self.redact_sensitive = redact;
        self
    }

    /// 设置元数据选项
    pub fn with_metadata(mut self, include: bool) -> Self {
        self.include_metadata = include;
        self
    }
}

/// 导出模式
#[derive(Debug, Clone, Default)]
pub struct ExportSchema {
    /// 字段定义
    pub fields: Vec<FieldDefinition>,
    /// 敏感字段列表
    pub sensitive_fields: Vec<String>,
    /// 必需字段列表
    pub required_fields: Vec<String>,
}

/// 字段定义
#[derive(Debug, Clone)]
pub struct FieldDefinition {
    /// 字段名
    pub name: String,
    /// 字段类型
    pub field_type: FieldType,
    /// 是否敏感
    pub is_sensitive: bool,
    /// 脱敏策略
    pub redaction_strategy: Option<RedactionStrategy>,
}

/// 字段类型
#[derive(Debug, Clone)]
pub enum FieldType {
    String,
    Number,
    Boolean,
    DateTime,
    Array(Box<FieldType>),
    Object(HashMap<String, FieldType>),
}

/// 脱敏策略
#[derive(Debug, Clone)]
pub enum RedactionStrategy {
    /// 完全隐藏
    Hide,
    /// 部分隐藏（如只显示后4位）
    Mask { show_first: usize, show_last: usize },
    /// 替换为固定值
    Replace(String),
    /// 哈希处理
    Hash,
}

/// 导出结果
#[derive(Debug, Clone)]
pub struct ExportResult {
    /// 导出数据
    pub data: Vec<u8>,
    /// 导出格式
    pub format: ExportFormat,
    /// 数字签名
    pub signature: Option<ExportSignature>,
    /// 导出清单
    pub manifest: ExportManifest,
    /// 会话 ID
    pub session_id: SessionId,
}

/// 导出签名
#[derive(Debug, Clone)]
pub struct ExportSignature {
    /// 签名算法
    pub algorithm: String,
    /// 签名数据（Base64）
    pub signature: String,
    /// 密钥 ID
    pub key_id: String,
    /// 签名时间
    pub timestamp: OffsetDateTime,
}

/// 导出清单
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportManifest {
    /// 导出 ID
    pub export_id: String,
    /// 导出时间
    pub exported_at: OffsetDateTime,
    /// 数据哈希
    pub data_hash: String,
    /// 格式版本
    pub format_version: String,
    /// 脱敏记录
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub redaction_log: Vec<RedactionRecord>,
    /// 元数据
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// 脱敏记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionRecord {
    /// 字段路径
    pub field_path: String,
    /// 原始类型
    pub original_type: String,
    /// 应用的脱敏策略
    pub strategy: String,
    /// 脱敏时间
    pub redacted_at: OffsetDateTime,
}

/// 导出服务配置
#[derive(Debug, Clone)]
pub struct ExportServiceConfig {
    /// 默认包含元数据
    pub default_include_metadata: bool,
    /// 默认脱敏敏感字段
    pub default_redact_sensitive: bool,
    /// JSON 美化输出
    pub json_pretty: bool,
    /// CSV 分隔符
    pub csv_delimiter: u8,
    /// CSV 包含标题
    pub csv_header: bool,
}

impl Default for ExportServiceConfig {
    fn default() -> Self {
        Self {
            default_include_metadata: true,
            default_redact_sensitive: true,
            json_pretty: true,
            csv_delimiter: b',',
            csv_header: true,
        }
    }
}

/// 数据导出服务
///
/// 提供结构化的安全数据导出功能
#[allow(dead_code)]
pub struct ExportService {
    redaction_service: Arc<RedactionService>,
    key_manager: Option<Arc<EnclaveKeyManager>>,
    config: ExportServiceConfig,
}

impl ExportService {
    /// 创建新的导出服务
    pub fn new(
        redaction_service: Arc<RedactionService>,
        key_manager: Option<Arc<EnclaveKeyManager>>,
        config: ExportServiceConfig,
    ) -> Self {
        Self {
            redaction_service,
            key_manager,
            config,
        }
    }

    /// 创建带默认配置的导出服务
    pub fn with_redaction_service(redaction_service: Arc<RedactionService>) -> Self {
        Self::new(redaction_service, None, ExportServiceConfig::default())
    }

    /// 创建带签名功能的导出服务
    pub fn with_key_manager(
        redaction_service: Arc<RedactionService>,
        key_manager: Arc<EnclaveKeyManager>,
    ) -> Self {
        Self::new(
            redaction_service,
            Some(key_manager),
            ExportServiceConfig::default(),
        )
    }

    /// 导出数据
    ///
    /// # Arguments
    ///
    /// * `request` - 导出请求
    /// * `session_id` - 会话 ID
    ///
    /// # Returns
    ///
    /// 返回导出结果，包含数据和签名
    pub async fn export(
        &self,
        request: ExportRequest,
        session_id: SessionId,
    ) -> Result<ExportResult, ExportError> {
        let start_time = OffsetDateTime::now_utc();
        info!(
            "开始导出数据: format={}, session={}",
            request.format, session_id
        );

        // 步骤 1: 脱敏处理（如果需要）
        let (processed_data, redaction_log) = if request.redact_sensitive {
            self.apply_redaction(&request.data, &request.schema).await?
        } else {
            (request.data.clone(), Vec::new())
        };

        // 步骤 2: 序列化为目标格式
        let serialized = self.serialize(&processed_data, request.format, &request)?;

        // 步骤 3: 计算数据哈希
        let data_hash = self.compute_hash(&serialized);

        // 步骤 4: 生成清单
        let manifest = ExportManifest {
            export_id: uuid::Uuid::new_v4().to_string(),
            exported_at: OffsetDateTime::now_utc(),
            data_hash: data_hash.clone(),
            format_version: "1.0".to_string(),
            redaction_log,
            metadata: if request.include_metadata {
                self.generate_metadata(&request)
            } else {
                HashMap::new()
            },
        };

        // 步骤 5: 组装最终数据（数据 + 清单）
        let final_data = self.assemble_export(&serialized, &manifest, request.format)?;

        // 步骤 6: 签名（如果配置了 EnclaveKeyManager）
        let signature = if let Some(ref km) = self.key_manager {
            match km.sign(&final_data).await {
                Ok(sig) => {
                    debug!("导出数据签名完成: key_id={}", sig.key_id);
                    use base64::Engine;
                    Some(ExportSignature {
                        algorithm: sig.algorithm.clone(),
                        signature: base64::engine::general_purpose::STANDARD.encode(&sig.data),
                        key_id: sig.key_id.clone(),
                        timestamp: sig.timestamp,
                    })
                }
                Err(e) => {
                    warn!("导出数据签名失败: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let execution_time_ms =
            (OffsetDateTime::now_utc() - start_time).whole_milliseconds() as u64;

        info!(
            "导出完成: {} bytes, {}ms, signed={}",
            final_data.len(),
            execution_time_ms,
            signature.is_some()
        );

        Ok(ExportResult {
            data: final_data,
            format: request.format,
            signature,
            manifest,
            session_id,
        })
    }

    /// 验证导出结果
    ///
    /// 验证导出数据的签名是否有效
    pub async fn verify_export(&self, export: &ExportResult) -> Result<bool, ExportError> {
        let Some(ref signature) = export.signature else {
            return Err(ExportError::Verification("导出结果没有签名".to_string()));
        };

        let Some(ref km) = self.key_manager else {
            return Err(ExportError::Verification("没有配置密钥管理器".to_string()));
        };

        // 重建签名对象
        use base64::Engine;
        let sig_data = base64::engine::general_purpose::STANDARD
            .decode(&signature.signature)
            .map_err(|e| ExportError::Verification(format!("签名解码失败: {}", e)))?;

        let sig = Signature {
            data: sig_data,
            algorithm: signature.algorithm.clone(),
            key_id: signature.key_id.clone(),
            timestamp: signature.timestamp,
            public_key: Vec::new(), // 验证时从 key_manager 获取
        };

        km.verify(&export.data, &sig)
            .await
            .map_err(|e| ExportError::Verification(format!("签名验证失败: {}", e)))
    }

    /// 应用脱敏
    async fn apply_redaction(
        &self,
        data: &serde_json::Value,
        schema: &ExportSchema,
    ) -> Result<(serde_json::Value, Vec<RedactionRecord>), ExportError> {
        let mut redacted = data.clone();
        let mut redaction_log = Vec::new();

        for field in &schema.sensitive_fields {
            if let Some(value) = redacted.pointer_mut(&format!("/{}", field.replace('.', "/")))
                && let Some(redacted_value) = self.redact_value(value)
            {
                redaction_log.push(RedactionRecord {
                    field_path: field.clone(),
                    original_type: Self::get_json_type(value),
                    strategy: "hide".to_string(),
                    redacted_at: OffsetDateTime::now_utc(),
                });
                *value = redacted_value;
            }
        }

        Ok((redacted, redaction_log))
    }

    /// 脱敏单个值
    fn redact_value(&self, value: &serde_json::Value) -> Option<serde_json::Value> {
        match value {
            serde_json::Value::String(s) => {
                if s.len() > 4 {
                    let redacted = format!("{}****", &s[..s.len().min(4)]);
                    Some(serde_json::Value::String(redacted))
                } else {
                    Some(serde_json::Value::String("****".to_string()))
                }
            }
            serde_json::Value::Number(_) => {
                Some(serde_json::Value::String("[REDACTED]".to_string()))
            }
            _ => None,
        }
    }

    /// 序列化数据
    fn serialize(
        &self,
        data: &serde_json::Value,
        format: ExportFormat,
        request: &ExportRequest,
    ) -> Result<Vec<u8>, ExportError> {
        match format {
            ExportFormat::Json => self.serialize_json(data),
            ExportFormat::Csv => self.serialize_csv(data, request),
            ExportFormat::Xml => self.serialize_xml(data),
            ExportFormat::Pdf => Err(ExportError::Redaction(
                "PDF export not implemented".to_string(),
            )),
        }
    }

    /// 序列化为 JSON
    fn serialize_json(&self, data: &serde_json::Value) -> Result<Vec<u8>, ExportError> {
        let json_str = if self.config.json_pretty {
            serde_json::to_string_pretty(data)
        } else {
            serde_json::to_string(data)
        }
        .map_err(|e| ExportError::Serialization(e.to_string()))?;

        Ok(json_str.into_bytes())
    }

    /// 将 JSON 值转换为 CSV 单元格字符串
    ///
    /// 字符串直接返回，对象/数组序列化为 JSON 字符串，其他类型调用 to_string()
    fn json_value_to_csv_cell(v: &serde_json::Value) -> String {
        match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => String::new(),
            // 嵌套对象/数组：序列化为 JSON 字符串放入单元格
            serde_json::Value::Object(_) | serde_json::Value::Array(_) => v.to_string(),
            other => other.to_string(),
        }
    }

    /// 序列化为 CSV
    ///
    /// 支持以下数据结构：
    /// - 对象数组（每个对象为一行，键为列名）
    /// - 单个对象（单行）
    /// - 混合类型数组（非对象元素整体序列化为单列字符串）
    /// - 标量值（单列单行）
    fn serialize_csv(
        &self,
        data: &serde_json::Value,
        _request: &ExportRequest,
    ) -> Result<Vec<u8>, ExportError> {
        let mut wtr = csv::WriterBuilder::new()
            .delimiter(self.config.csv_delimiter)
            .from_writer(Vec::new());

        match data {
            serde_json::Value::Array(arr) => {
                // 判断是否为纯对象数组（用第一个非 null 元素推断）
                let first_obj = arr
                    .iter()
                    .find(|v| v.is_object())
                    .and_then(|v| v.as_object());

                if first_obj.is_some() {
                    // 对象数组路径：收集所有对象的所有键作为统一标题行
                    // 修复：处理不同 Schema 的对象，避免列错位
                    let mut all_keys: Vec<String> = Vec::new();
                    let mut key_set = std::collections::HashSet::new();

                    // 第一遍：收集所有可能的键
                    for item in arr.iter() {
                        if let serde_json::Value::Object(obj) = item {
                            for key in obj.keys() {
                                if key_set.insert(key.clone()) {
                                    all_keys.push(key.clone());
                                }
                            }
                        }
                    }

                    // 如果没有找到任何键（理论上不会发生），降级为标量处理
                    if all_keys.is_empty() {
                        if self.config.csv_header {
                            wtr.write_record(["value"])
                                .map_err(|e| ExportError::Serialization(e.to_string()))?;
                        }
                        for item in arr {
                            wtr.write_record(&[Self::json_value_to_csv_cell(item)])
                                .map_err(|e| ExportError::Serialization(e.to_string()))?;
                        }
                    } else {
                        // 写入统一标题行
                        if self.config.csv_header {
                            wtr.write_record(&all_keys)
                                .map_err(|e| ExportError::Serialization(e.to_string()))?;
                        }

                        // 第二遍：按统一顺序写入每一行
                        for item in arr {
                            if let serde_json::Value::Object(obj) = item {
                                let row: Vec<String> = all_keys
                                    .iter()
                                    .map(|key| {
                                        obj.get(key)
                                            .map(Self::json_value_to_csv_cell)
                                            .unwrap_or_default()
                                    })
                                    .collect();
                                wtr.write_record(&row)
                                    .map_err(|e| ExportError::Serialization(e.to_string()))?;
                            } else {
                                // 混合类型数组中的非对象元素：按空值填充其他列，第一列为该值
                                let mut row: Vec<String> = vec![Self::json_value_to_csv_cell(item)];
                                row.extend(std::iter::repeat_n(String::new(), all_keys.len() - 1));
                                wtr.write_record(&row)
                                    .map_err(|e| ExportError::Serialization(e.to_string()))?;
                            }
                        }
                    }
                } else {
                    // 纯非对象数组（标量/嵌套数组）：单列输出
                    if self.config.csv_header {
                        wtr.write_record(["value"])
                            .map_err(|e| ExportError::Serialization(e.to_string()))?;
                    }
                    for item in arr {
                        wtr.write_record([Self::json_value_to_csv_cell(item)])
                            .map_err(|e| ExportError::Serialization(e.to_string()))?;
                    }
                }
            }
            serde_json::Value::Object(obj) => {
                if self.config.csv_header {
                    let headers: Vec<_> = obj.keys().cloned().collect();
                    wtr.write_record(&headers)
                        .map_err(|e| ExportError::Serialization(e.to_string()))?;
                }
                let row: Vec<String> = obj.values().map(Self::json_value_to_csv_cell).collect();
                wtr.write_record(&row)
                    .map_err(|e| ExportError::Serialization(e.to_string()))?;
            }
            // 标量值：单列单行
            scalar => {
                if self.config.csv_header {
                    wtr.write_record(["value"])
                        .map_err(|e| ExportError::Serialization(e.to_string()))?;
                }
                wtr.write_record([Self::json_value_to_csv_cell(scalar)])
                    .map_err(|e| ExportError::Serialization(e.to_string()))?;
            }
        }

        wtr.into_inner()
            .map_err(|e| ExportError::Serialization(e.to_string()))
    }

    /// 序列化为 XML
    fn serialize_xml(&self, data: &serde_json::Value) -> Result<Vec<u8>, ExportError> {
        // 简化实现：将 JSON 转换为基本 XML
        let mut xml = String::new();
        xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        xml.push_str("<export>\n");

        self.json_to_xml(data, &mut xml, 1);

        xml.push_str("</export>\n");
        Ok(xml.into_bytes())
    }

    /// JSON 转 XML
    fn json_to_xml(&self, value: &serde_json::Value, xml: &mut String, indent: usize) {
        let indent_str = "  ".repeat(indent);

        match value {
            serde_json::Value::Object(obj) => {
                for (key, val) in obj {
                    xml.push_str(&format!("{}<{}>", indent_str, key));
                    if val.is_object() || val.is_array() {
                        xml.push('\n');
                        self.json_to_xml(val, xml, indent + 1);
                        xml.push_str(&format!("{}</{}>\n", indent_str, key));
                    } else {
                        xml.push_str(&escape_xml(&val.to_string()));
                        xml.push_str(&format!("</{}>\n", key));
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    xml.push_str(&format!("{}<item>\n", indent_str));
                    self.json_to_xml(item, xml, indent + 1);
                    xml.push_str(&format!("{}</item>\n", indent_str));
                }
            }
            serde_json::Value::String(s) => {
                xml.push_str(&escape_xml(s));
            }
            other => {
                xml.push_str(&escape_xml(&other.to_string()));
            }
        }
    }

    /// 计算数据哈希
    fn compute_hash(&self, data: &[u8]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// 生成元数据
    fn generate_metadata(&self, request: &ExportRequest) -> HashMap<String, serde_json::Value> {
        let mut metadata = HashMap::new();
        metadata.insert(
            "format".to_string(),
            serde_json::Value::String(request.format.to_string()),
        );
        metadata.insert(
            "redacted".to_string(),
            serde_json::Value::Bool(request.redact_sensitive),
        );
        metadata
    }

    /// 组装导出数据
    fn assemble_export(
        &self,
        data: &[u8],
        manifest: &ExportManifest,
        format: ExportFormat,
    ) -> Result<Vec<u8>, ExportError> {
        match format {
            ExportFormat::Json => {
                let mut result = data.to_vec();
                result.extend_from_slice(b"\n\n// MANIFEST\n");
                let manifest_json = serde_json::to_string_pretty(manifest)
                    .map_err(|e| ExportError::Serialization(e.to_string()))?;
                result.extend_from_slice(manifest_json.as_bytes());
                Ok(result)
            }
            _ => Ok(data.to_vec()),
        }
    }

    /// 获取 JSON 类型名称
    fn get_json_type(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Null => "null".to_string(),
            serde_json::Value::Bool(_) => "boolean".to_string(),
            serde_json::Value::Number(_) => "number".to_string(),
            serde_json::Value::String(_) => "string".to_string(),
            serde_json::Value::Array(_) => "array".to_string(),
            serde_json::Value::Object(_) => "object".to_string(),
        }
    }
}

/// XML 转义
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_redaction_service() -> Arc<RedactionService> {
        Arc::new(RedactionService::new())
    }

    #[tokio::test]
    async fn test_export_json() {
        let service = ExportService::with_redaction_service(create_test_redaction_service());

        let data = serde_json::json!({
            "name": "John Doe",
            "email": "john@example.com"
        });

        let request = ExportRequest::new(ExportFormat::Json, data).with_redaction(false);

        let result = service.export(request, SessionId::new()).await;
        assert!(result.is_ok());

        let export = result.unwrap();
        assert_eq!(export.format, ExportFormat::Json);
    }

    #[tokio::test]
    async fn test_export_csv() {
        let service = ExportService::with_redaction_service(create_test_redaction_service());

        let data = serde_json::json!([
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 25}
        ]);

        let request = ExportRequest::new(ExportFormat::Csv, data).with_redaction(false);

        let result = service.export(request, SessionId::new()).await;
        assert!(result.is_ok());

        let export = result.unwrap();
        assert_eq!(export.format, ExportFormat::Csv);
    }

    #[test]
    fn test_export_format_display() {
        assert_eq!(ExportFormat::Json.to_string(), "json");
        assert_eq!(ExportFormat::Csv.to_string(), "csv");
        assert_eq!(ExportFormat::Pdf.mime_type(), "application/pdf");
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("<script>"), "&lt;script&gt;");
        assert_eq!(escape_xml("\"test\""), "&quot;test&quot;");
        assert_eq!(escape_xml("A & B"), "A &amp; B");
    }

    #[test]
    fn test_export_request_builder() {
        let request = ExportRequest::new(ExportFormat::Json, serde_json::json!({}))
            .with_redaction(true)
            .with_metadata(false);

        assert!(request.redact_sensitive);
        assert!(!request.include_metadata);
    }
}
