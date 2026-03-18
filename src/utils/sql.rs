//! SQL 工具函数

/// SQL 字符串转义（防止 SQL 注入）
///
/// 用于 PostgreSQL SET 命令的会话变量设置（不支持参数化查询）
pub fn escape_sql_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\0', "\\0")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_escape_basic() {
        assert_eq!(escape_sql_string("test"), "test");
    }

    #[test]
    fn test_sql_escape_quotes() {
        assert_eq!(
            escape_sql_string("test' OR '1'='1"),
            "test\\' OR \\'1\\'=\\'1"
        );
    }

    #[test]
    fn test_sql_escape_backslash() {
        assert_eq!(escape_sql_string("test\\value"), "test\\\\value");
    }

    #[test]
    fn test_sql_escape_newline() {
        assert_eq!(escape_sql_string("line1\nline2"), "line1\\nline2");
    }

    #[test]
    fn test_sql_escape_null() {
        assert_eq!(escape_sql_string("test\0end"), "test\\0end");
    }

    #[test]
    fn test_sql_escape_unicode() {
        assert_eq!(escape_sql_string("测试'quote"), "测试\\'quote");
    }
}
