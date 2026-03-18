//! 安全导出模块
//!
//! 提供安全的截图导出和数据导出功能，包括：
//! - 页面状态冻结（TOCTOU 防护）
//! - 安全截图
//! - 内容审核（Vision API）
//! - 敏感信息脱敏
//! - 数字签名

pub mod data_export;
pub mod freezer;
pub mod redaction;
pub mod review;
pub mod screenshot;
pub mod watermark;

// 重新导出主要类型
pub use data_export::{ExportFormat, ExportRequest, ExportResult, ExportService};
pub use freezer::{FreezeGuard, FreezeState, FrozenPageState, PageStateFreezer};
pub use redaction::RedactionService;
pub use review::{
    ContentReviewer, RedactionAction, RedactionRegion, RegionType, ReviewConfig, ReviewResult,
    RiskLevel, SensitiveItem, SensitiveType,
};
pub use screenshot::{
    ImageDimensions, ImageFormat, ScreenshotConfig, ScreenshotRequest, ScreenshotResult,
    ScreenshotService,
};
pub use watermark::{WatermarkConfig, WatermarkMetadata, WatermarkPosition, WatermarkService};
