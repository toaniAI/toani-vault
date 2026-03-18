//! 参数验证器模块
//!
//! 提供灵活的参数验证机制，支持 JSON Schema 验证和自定义验证规则。

use crate::connector::error::ValidationError;
use regex::Regex;
use serde_json::Value;

// ============================================================================
// ValidationRule Trait
// ============================================================================

/// 验证规则 trait
///
/// 所有自定义验证规则都需要实现此 trait。
///
/// # 示例
///
/// ```rust
/// use vault_service::connector::validator::ValidationRule;
/// use serde_json::Value;
///
/// struct MyRule;
///
/// impl ValidationRule for MyRule {
///     fn name(&self) -> &'static str {
///         "my_rule"
///     }
///
///     fn validate(&self, value: &Value) -> Result<(), String> {
///         // 验证逻辑
///         Ok(())
///     }
/// }
/// ```
pub trait ValidationRule: Send + Sync {
    /// 规则名称
    fn name(&self) -> &'static str;

    /// 验证值
    fn validate(&self, value: &Value) -> Result<(), String>;

    /// 获取错误代码（可选）
    fn error_code(&self) -> Option<&'static str> {
        None
    }
}

// ============================================================================
// SchemaValidator
// ============================================================================

/// JSON Schema 验证器
///
/// 基于 JSON Schema 进行验证（简化实现）。
///
/// # 支持的 Schema 关键字
///
/// - `type`: 类型检查（string, number, integer, boolean, array, object, null）
/// - `required`: 必需字段检查
/// - `properties`: 对象属性验证
/// - `items`: 数组元素验证
/// - `minimum`/`maximum`: 数值范围
/// - `minLength`/`maxLength`: 字符串长度
/// - `pattern`: 正则表达式匹配
/// - `enum`: 枚举值检查
pub struct SchemaValidator {
    schema: Value,
}

impl SchemaValidator {
    /// 创建新的 Schema 验证器
    pub fn new(schema: Value) -> Self {
        Self { schema }
    }

    /// 验证值是否符合 Schema
    pub fn validate(&self, value: &Value) -> Result<(), ValidationError> {
        self.validate_against_schema(value, &self.schema, "")
    }

    /// 根据 Schema 验证值
    fn validate_against_schema(
        &self,
        value: &Value,
        schema: &Value,
        path: &str,
    ) -> Result<(), ValidationError> {
        // 检查 type
        if let Some(type_str) = schema.get("type").and_then(|t| t.as_str()) {
            self.validate_type(value, type_str, path)?;
        }

        // 检查 required（仅对对象）
        if let Some(required) = schema.get("required").and_then(|r| r.as_array())
            && let Value::Object(obj) = value
        {
            for field in required {
                if let Some(field_name) = field.as_str()
                    && !obj.contains_key(field_name)
                {
                    return Err(ValidationError::with_code(
                        Some(format!("{}{}", path, field_name)),
                        format!("缺少必需字段: {}", field_name),
                        "REQUIRED_FIELD",
                    ));
                }
            }
        }

        // 检查 properties（仅对对象）
        if let Some(properties) = schema.get("properties").and_then(|p| p.as_object())
            && let Value::Object(obj) = value
        {
            for (prop_name, prop_schema) in properties {
                if let Some(prop_value) = obj.get(prop_name) {
                    let prop_path = format!("{}{}.", path, prop_name);
                    self.validate_against_schema(prop_value, prop_schema, &prop_path)?;
                }
            }
        }

        // 检查 items（仅对数组）
        if let Some(items_schema) = schema.get("items")
            && let Value::Array(arr) = value
        {
            for (i, item) in arr.iter().enumerate() {
                let item_path = format!("{}[{}].", path, i);
                self.validate_against_schema(item, items_schema, &item_path)?;
            }
        }

        // 检查 minimum（数值最小值）
        if let Some(min) = schema.get("minimum").and_then(|m| m.as_f64())
            && let Value::Number(n) = value
            && let Some(v) = n.as_f64()
            && v < min
        {
            return Err(ValidationError::with_code(
                Some(path.to_string()),
                format!("值 {} 小于最小值 {}", v, min),
                "BELOW_MINIMUM",
            ));
        }

        // 检查 maximum（数值最大值）
        if let Some(max) = schema.get("maximum").and_then(|m| m.as_f64())
            && let Value::Number(n) = value
            && let Some(v) = n.as_f64()
            && v > max
        {
            return Err(ValidationError::with_code(
                Some(path.to_string()),
                format!("值 {} 大于最大值 {}", v, max),
                "ABOVE_MAXIMUM",
            ));
        }

        // 检查 minLength（字符串最小长度）
        if let Some(min_len) = schema.get("minLength").and_then(|m| m.as_u64())
            && let Value::String(s) = value
            && s.len() < min_len as usize
        {
            return Err(ValidationError::with_code(
                Some(path.to_string()),
                format!("字符串长度 {} 小于最小长度 {}", s.len(), min_len),
                "BELOW_MIN_LENGTH",
            ));
        }

        // 检查 maxLength（字符串最大长度）
        if let Some(max_len) = schema.get("maxLength").and_then(|m| m.as_u64())
            && let Value::String(s) = value
            && s.len() > max_len as usize
        {
            return Err(ValidationError::with_code(
                Some(path.to_string()),
                format!("字符串长度 {} 大于最大长度 {}", s.len(), max_len),
                "ABOVE_MAX_LENGTH",
            ));
        }

        // 检查 pattern（正则表达式）
        if let Some(pattern) = schema.get("pattern").and_then(|p| p.as_str())
            && let Value::String(s) = value
            && let Ok(regex) = Regex::new(pattern)
            && !regex.is_match(s)
        {
            return Err(ValidationError::with_code(
                Some(path.to_string()),
                format!("字符串 '{}' 不匹配模式 '{}'", s, pattern),
                "PATTERN_MISMATCH",
            ));
        }

        // 检查 enum（枚举值）
        if let Some(enum_values) = schema.get("enum").and_then(|e| e.as_array())
            && !enum_values.contains(value)
        {
            return Err(ValidationError::with_code(
                Some(path.to_string()),
                format!("值 {} 不在允许的枚举值中: {:?}", value, enum_values),
                "INVALID_ENUM",
            ));
        }

        Ok(())
    }

    /// 验证类型
    fn validate_type(
        &self,
        value: &Value,
        type_str: &str,
        path: &str,
    ) -> Result<(), ValidationError> {
        let valid = match type_str {
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.is_i64() || value.is_u64(),
            "boolean" => value.is_boolean(),
            "array" => value.is_array(),
            "object" => value.is_object(),
            "null" => value.is_null(),
            _ => true, // 未知类型，跳过验证
        };

        if !valid {
            return Err(ValidationError::with_code(
                Some(path.to_string()),
                format!("类型错误：期望 {}, 实际 {:?}", type_str, value),
                "TYPE_MISMATCH",
            ));
        }

        Ok(())
    }
}

// ============================================================================
// CompositeValidator
// ============================================================================

/// 组合验证器
///
/// 组合 Schema 验证和多个自定义验证规则。
pub struct CompositeValidator {
    /// Schema 验证器
    schema: Option<SchemaValidator>,
    /// 自定义验证规则
    rules: Vec<Box<dyn ValidationRule>>,
}

impl CompositeValidator {
    /// 创建新的组合验证器
    pub fn new() -> Self {
        Self {
            schema: None,
            rules: Vec::new(),
        }
    }

    /// 添加 Schema 验证
    pub fn with_schema(mut self, schema: Value) -> Self {
        self.schema = Some(SchemaValidator::new(schema));
        self
    }

    /// 添加验证规则
    pub fn with_rule(mut self, rule: Box<dyn ValidationRule>) -> Self {
        self.rules.push(rule);
        self
    }

    /// 验证值
    pub fn validate(&self, value: &Value) -> Result<(), ValidationError> {
        // 首先进行 Schema 验证
        if let Some(schema) = &self.schema {
            schema.validate(value)?;
        }

        // 然后执行自定义规则
        for rule in &self.rules {
            if let Err(msg) = rule.validate(value) {
                return Err(ValidationError::with_code(
                    None,
                    msg,
                    rule.error_code().unwrap_or("VALIDATION_FAILED"),
                ));
            }
        }

        Ok(())
    }
}

impl Default for CompositeValidator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// ValidatorBuilder
// ============================================================================

/// 验证器构建器
///
/// 用于链式构建复合验证器。
///
/// # 示例
///
/// ```rust
/// use vault_service::connector::validator::{
///     ValidatorBuilder, rules::RequiredRule
/// };
/// use serde_json::json;
///
/// let validator = ValidatorBuilder::new()
///     .with_schema(json!({
///         "type": "object",
///         "required": ["name"]
///     }))
///     .build();
///
/// let result = validator.validate(&json!({"name": "test"}));
/// assert!(result.is_ok());
/// ```
pub struct ValidatorBuilder {
    schema: Option<Value>,
    rules: Vec<Box<dyn ValidationRule>>,
}

impl ValidatorBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self {
            schema: None,
            rules: Vec::new(),
        }
    }

    /// 设置 JSON Schema
    pub fn with_schema(mut self, schema: Value) -> Self {
        self.schema = Some(schema);
        self
    }

    /// 添加验证规则
    pub fn with_rule(mut self, rule: Box<dyn ValidationRule>) -> Self {
        self.rules.push(rule);
        self
    }

    /// 构建组合验证器
    pub fn build(self) -> CompositeValidator {
        let mut validator = CompositeValidator::new();

        if let Some(schema) = self.schema {
            validator = validator.with_schema(schema);
        }

        for rule in self.rules {
            validator = validator.with_rule(rule);
        }

        validator
    }
}

impl Default for ValidatorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 预定义验证规则
// ============================================================================

pub mod rules {
    use super::{ValidationRule, Value};
    use regex::Regex;

    /// 必需字段规则
    pub struct RequiredRule {
        fields: Vec<String>,
    }

    impl RequiredRule {
        pub fn new(fields: Vec<&str>) -> Self {
            Self {
                fields: fields.into_iter().map(|s| s.to_string()).collect(),
            }
        }
    }

    impl ValidationRule for RequiredRule {
        fn name(&self) -> &'static str {
            "required"
        }

        fn validate(&self, value: &Value) -> Result<(), String> {
            if let Value::Object(obj) = value {
                for field in &self.fields {
                    if !obj.contains_key(field) {
                        return Err(format!("缺少必需字段: {}", field));
                    }
                }
            }
            Ok(())
        }

        fn error_code(&self) -> Option<&'static str> {
            Some("REQUIRED_FIELD")
        }
    }

    /// 类型检查规则
    pub struct TypeRule {
        field: String,
        expected_type: String,
    }

    impl TypeRule {
        pub fn new(field: impl Into<String>, expected_type: impl Into<String>) -> Self {
            Self {
                field: field.into(),
                expected_type: expected_type.into(),
            }
        }

        fn check_type(&self, value: &Value) -> bool {
            match self.expected_type.as_str() {
                "string" => value.is_string(),
                "number" => value.is_number(),
                "integer" => value.is_i64() || value.is_u64(),
                "boolean" => value.is_boolean(),
                "array" => value.is_array(),
                "object" => value.is_object(),
                "null" => value.is_null(),
                _ => true,
            }
        }
    }

    impl ValidationRule for TypeRule {
        fn name(&self) -> &'static str {
            "type"
        }

        fn validate(&self, value: &Value) -> Result<(), String> {
            if let Value::Object(obj) = value
                && let Some(field_value) = obj.get(&self.field)
                && !self.check_type(field_value)
            {
                return Err(format!(
                    "字段 '{}' 类型错误，期望 '{}'",
                    self.field, self.expected_type
                ));
            }
            Ok(())
        }

        fn error_code(&self) -> Option<&'static str> {
            Some("TYPE_MISMATCH")
        }
    }

    /// 最小长度规则
    pub struct MinLengthRule {
        field: String,
        min_length: usize,
    }

    impl MinLengthRule {
        pub fn new(field: impl Into<String>, min_length: usize) -> Self {
            Self {
                field: field.into(),
                min_length,
            }
        }
    }

    impl ValidationRule for MinLengthRule {
        fn name(&self) -> &'static str {
            "min_length"
        }

        fn validate(&self, value: &Value) -> Result<(), String> {
            if let Value::Object(obj) = value
                && let Some(Value::String(s)) = obj.get(&self.field)
                && s.len() < self.min_length
            {
                return Err(format!(
                    "字段 '{}' 长度 {} 小于最小长度 {}",
                    self.field,
                    s.len(),
                    self.min_length
                ));
            }
            Ok(())
        }

        fn error_code(&self) -> Option<&'static str> {
            Some("BELOW_MIN_LENGTH")
        }
    }

    /// 最大长度规则
    pub struct MaxLengthRule {
        field: String,
        max_length: usize,
    }

    impl MaxLengthRule {
        pub fn new(field: impl Into<String>, max_length: usize) -> Self {
            Self {
                field: field.into(),
                max_length,
            }
        }
    }

    impl ValidationRule for MaxLengthRule {
        fn name(&self) -> &'static str {
            "max_length"
        }

        fn validate(&self, value: &Value) -> Result<(), String> {
            if let Value::Object(obj) = value
                && let Some(Value::String(s)) = obj.get(&self.field)
                && s.len() > self.max_length
            {
                return Err(format!(
                    "字段 '{}' 长度 {} 大于最大长度 {}",
                    self.field,
                    s.len(),
                    self.max_length
                ));
            }
            Ok(())
        }

        fn error_code(&self) -> Option<&'static str> {
            Some("ABOVE_MAX_LENGTH")
        }
    }

    /// 正则表达式规则
    pub struct PatternRule {
        field: String,
        pattern: Regex,
        pattern_str: String,
    }

    impl PatternRule {
        pub fn new(field: impl Into<String>, pattern: impl Into<String>) -> Result<Self, String> {
            let pattern_str = pattern.into();
            let regex = Regex::new(&pattern_str).map_err(|e| format!("无效的正则表达式: {}", e))?;
            Ok(Self {
                field: field.into(),
                pattern: regex,
                pattern_str,
            })
        }
    }

    impl ValidationRule for PatternRule {
        fn name(&self) -> &'static str {
            "pattern"
        }

        fn validate(&self, value: &Value) -> Result<(), String> {
            if let Value::Object(obj) = value
                && let Some(Value::String(s)) = obj.get(&self.field)
                && !self.pattern.is_match(s)
            {
                return Err(format!(
                    "字段 '{}' 值 '{}' 不匹配模式 '{}'",
                    self.field, s, self.pattern_str
                ));
            }
            Ok(())
        }

        fn error_code(&self) -> Option<&'static str> {
            Some("PATTERN_MISMATCH")
        }
    }

    /// 范围规则
    pub struct RangeRule {
        field: String,
        min: Option<f64>,
        max: Option<f64>,
    }

    impl RangeRule {
        pub fn new(field: impl Into<String>) -> Self {
            Self {
                field: field.into(),
                min: None,
                max: None,
            }
        }

        pub fn with_min(mut self, min: f64) -> Self {
            self.min = Some(min);
            self
        }

        pub fn with_max(mut self, max: f64) -> Self {
            self.max = Some(max);
            self
        }
    }

    impl ValidationRule for RangeRule {
        fn name(&self) -> &'static str {
            "range"
        }

        fn validate(&self, value: &Value) -> Result<(), String> {
            if let Value::Object(obj) = value
                && let Some(Value::Number(n)) = obj.get(&self.field)
                && let Some(v) = n.as_f64()
            {
                if let Some(min) = self.min
                    && v < min
                {
                    return Err(format!("字段 '{}' 值 {} 小于最小值 {}", self.field, v, min));
                }
                if let Some(max) = self.max
                    && v > max
                {
                    return Err(format!("字段 '{}' 值 {} 大于最大值 {}", self.field, v, max));
                }
            }
            Ok(())
        }

        fn error_code(&self) -> Option<&'static str> {
            Some("OUT_OF_RANGE")
        }
    }

    /// 枚举值规则
    pub struct EnumRule {
        field: String,
        allowed_values: Vec<Value>,
    }

    impl EnumRule {
        pub fn new(field: impl Into<String>, allowed_values: Vec<Value>) -> Self {
            Self {
                field: field.into(),
                allowed_values,
            }
        }
    }

    impl ValidationRule for EnumRule {
        fn name(&self) -> &'static str {
            "enum"
        }

        fn validate(&self, value: &Value) -> Result<(), String> {
            if let Value::Object(obj) = value
                && let Some(field_value) = obj.get(&self.field)
                && !self.allowed_values.contains(field_value)
            {
                return Err(format!(
                    "字段 '{}' 值 {:?} 不在允许值中",
                    self.field, field_value
                ));
            }
            Ok(())
        }

        fn error_code(&self) -> Option<&'static str> {
            Some("INVALID_ENUM")
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_schema_validator_type() {
        let schema = json!({
            "type": "object"
        });

        let validator = SchemaValidator::new(schema);

        // 正确类型
        assert!(validator.validate(&json!({"key": "value"})).is_ok());

        // 错误类型
        assert!(validator.validate(&json!("string")).is_err());
    }

    #[test]
    fn test_schema_validator_required() {
        let schema = json!({
            "type": "object",
            "required": ["name", "email"]
        });

        let validator = SchemaValidator::new(schema);

        // 所有必需字段存在
        assert!(
            validator
                .validate(&json!({"name": "test", "email": "test@example.com"}))
                .is_ok()
        );

        // 缺少必需字段
        assert!(validator.validate(&json!({"name": "test"})).is_err());
    }

    #[test]
    fn test_schema_validator_properties() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer" }
            }
        });

        let validator = SchemaValidator::new(schema);

        // 正确类型
        assert!(
            validator
                .validate(&json!({"name": "test", "age": 25}))
                .is_ok()
        );

        // 错误类型
        assert!(
            validator
                .validate(&json!({"name": 123, "age": 25}))
                .is_err()
        );
    }

    #[test]
    fn test_schema_validator_string_length() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "minLength": 3,
                    "maxLength": 10
                }
            }
        });

        let validator = SchemaValidator::new(schema);

        // 长度正确
        assert!(validator.validate(&json!({"name": "test"})).is_ok());

        // 太短
        assert!(validator.validate(&json!({"name": "ab"})).is_err());

        // 太长
        assert!(
            validator
                .validate(&json!({"name": "verylongname"}))
                .is_err()
        );
    }

    #[test]
    fn test_schema_validator_pattern() {
        let schema = json!({
            "type": "object",
            "properties": {
                "email": {
                    "type": "string",
                    "pattern": "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$"
                }
            }
        });

        let validator = SchemaValidator::new(schema);

        // 正确格式
        assert!(
            validator
                .validate(&json!({"email": "test@example.com"}))
                .is_ok()
        );

        // 错误格式
        assert!(validator.validate(&json!({"email": "invalid"})).is_err());
    }

    #[test]
    fn test_schema_validator_enum() {
        let schema = json!({
            "type": "object",
            "properties": {
                "status": {
                    "enum": ["active", "inactive", "pending"]
                }
            }
        });

        let validator = SchemaValidator::new(schema);

        // 有效枚举值
        assert!(validator.validate(&json!({"status": "active"})).is_ok());

        // 无效枚举值
        assert!(validator.validate(&json!({"status": "unknown"})).is_err());
    }

    #[test]
    fn test_composite_validator() {
        let validator = CompositeValidator::new()
            .with_schema(json!({
                "type": "object",
                "required": ["name"]
            }))
            .with_rule(Box::new(rules::MinLengthRule::new("name", 3)));

        // 有效值
        assert!(validator.validate(&json!({"name": "test"})).is_ok());

        // 缺少必需字段
        assert!(validator.validate(&json!({})).is_err());

        // 长度不足
        assert!(validator.validate(&json!({"name": "ab"})).is_err());
    }

    #[test]
    fn test_validator_builder() {
        let validator = ValidatorBuilder::new()
            .with_schema(json!({
                "type": "object",
                "required": ["email"]
            }))
            .with_rule(Box::new(
                rules::PatternRule::new("email", "^[^@]+@[^@]+$").unwrap(),
            ))
            .build();

        // 有效值
        assert!(
            validator
                .validate(&json!({"email": "test@example.com"}))
                .is_ok()
        );

        // 缺少必需字段
        assert!(validator.validate(&json!({})).is_err());

        // 格式不正确
        assert!(validator.validate(&json!({"email": "invalid"})).is_err());
    }

    #[test]
    fn test_required_rule() {
        let rule = rules::RequiredRule::new(vec!["name", "email"]);

        assert!(
            rule.validate(&json!({"name": "test", "email": "test@example.com"}))
                .is_ok()
        );
        assert!(rule.validate(&json!({"name": "test"})).is_err());
    }

    #[test]
    fn test_type_rule() {
        let rule = rules::TypeRule::new("age", "integer");

        assert!(rule.validate(&json!({"age": 25})).is_ok());
        assert!(rule.validate(&json!({"age": "25"})).is_err());
    }

    #[test]
    fn test_range_rule() {
        let rule = rules::RangeRule::new("age").with_min(0.0).with_max(150.0);

        assert!(rule.validate(&json!({"age": 25})).is_ok());
        assert!(rule.validate(&json!({"age": -1})).is_err());
        assert!(rule.validate(&json!({"age": 200})).is_err());
    }

    #[test]
    fn test_enum_rule() {
        let rule = rules::EnumRule::new(
            "status",
            vec![json!("active"), json!("inactive"), json!("pending")],
        );

        assert!(rule.validate(&json!({"status": "active"})).is_ok());
        assert!(rule.validate(&json!({"status": "unknown"})).is_err());
    }
}
