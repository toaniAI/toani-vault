use anyhow::Result;
use comfy_table::{Attribute, Cell, ContentArrangement, Table};
use serde::Serialize;

/// 输出格式化器
pub struct OutputFormatter {
    format: crate::config::OutputFormat,
}

impl OutputFormatter {
    pub fn new(format: crate::config::OutputFormat) -> Self {
        Self { format }
    }

    /// 打印单个对象
    pub fn print_object<T: Serialize>(&self, obj: &T) -> Result<()> {
        match self.format {
            crate::config::OutputFormat::Json => {
                let json = serde_json::to_string_pretty(obj)?;
                println!("{json}");
            }
            crate::config::OutputFormat::Table => {
                // 将对象转换为键值表格
                let mut table = Table::new();
                table.set_content_arrangement(ContentArrangement::Dynamic);
                table.set_header(vec!["Field", "Value"]);

                let json_value = serde_json::to_value(obj)?;
                if let serde_json::Value::Object(map) = json_value {
                    for (key, value) in map {
                        let value_str = format_value(&value);
                        table.add_row(vec![
                            Cell::new(key).add_attribute(Attribute::Bold),
                            Cell::new(value_str),
                        ]);
                    }
                }

                println!("{table}");
            }
        }
        Ok(())
    }

    /// 打印对象列表
    pub fn print_list<T: Serialize>(&self, items: &[T], columns: &[&str]) -> Result<()> {
        match self.format {
            crate::config::OutputFormat::Json => {
                let json = serde_json::to_string_pretty(items)?;
                println!("{json}");
            }
            crate::config::OutputFormat::Table => {
                let mut table = Table::new();
                table.set_content_arrangement(ContentArrangement::DynamicFullWidth);
                table.set_header(
                    columns
                        .iter()
                        .map(|c| Cell::new(*c).add_attribute(Attribute::Bold)),
                );

                for item in items {
                    let json_value = serde_json::to_value(item)?;
                    if let serde_json::Value::Object(map) = json_value {
                        let row: Vec<_> = columns
                            .iter()
                            .map(|col| {
                                let key = col.to_lowercase().replace(' ', "_");
                                let value = map.get(&key).unwrap_or(&serde_json::Value::Null);
                                Cell::new(format_value(value))
                            })
                            .collect();
                        table.add_row(row);
                    }
                }

                println!("{table}");
            }
        }
        Ok(())
    }

    /// 打印成功消息
    pub fn print_success(&self, message: &str) {
        match self.format {
            crate::config::OutputFormat::Json => {
                println!("{{\"status\":\"success\",\"message\":\"{message}\"}}");
            }
            crate::config::OutputFormat::Table => {
                println!("✅ {message}");
            }
        }
    }

    /// 打印错误消息
    pub fn print_error(&self, message: &str) {
        match self.format {
            crate::config::OutputFormat::Json => {
                eprintln!("{{\"status\":\"error\",\"message\":\"{message}\"}}");
            }
            crate::config::OutputFormat::Table => {
                eprintln!("❌ {message}");
            }
        }
    }

    /// 检查是否为表格格式
    pub fn is_table(&self) -> bool {
        matches!(self.format, crate::config::OutputFormat::Table)
    }

    /// 向 stderr 输出诊断信息
    pub fn print_diagnostic(&self, message: &str) {
        eprintln!("{message}");
    }
}

/// 格式化 JSON 值为字符串
fn format_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "-".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => {
            // 截断长字符串
            if s.len() > 100 {
                format!("{}...", &s[..97])
            } else {
                s.clone()
            }
        }
        serde_json::Value::Array(arr) => {
            if arr.is_empty() {
                "[]".to_string()
            } else {
                format!("[{} items]", arr.len())
            }
        }
        serde_json::Value::Object(obj) => {
            if obj.is_empty() {
                "{}".to_string()
            } else {
                format!("{{{} fields}}", obj.len())
            }
        }
    }
}

/// 打印原始 JSON
pub fn print_raw_json<T: Serialize>(obj: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(obj)?;
    println!("{json}");
    Ok(())
}

/// 确认提示
pub fn confirm(message: &str) -> Result<bool> {
    use dialoguer::Confirm;

    Ok(Confirm::new()
        .with_prompt(message)
        .default(false)
        .interact()?)
}

/// 输入提示 (敏感信息)
pub fn input_secret(prompt: &str) -> Result<String> {
    use dialoguer::Password;

    Ok(Password::new().with_prompt(prompt).interact()?)
}

/// 输入提示 (普通)
pub fn input_text(prompt: &str) -> Result<String> {
    use dialoguer::Input;

    Ok(Input::new().with_prompt(prompt).interact()?)
}

/// 选择提示
#[allow(dead_code)]
pub fn select<T: ToString>(prompt: &str, items: &[T]) -> Result<usize> {
    use dialoguer::Select;

    Ok(Select::new().with_prompt(prompt).items(items).interact()?)
}
