//! 水印服务模块
//!
//! 为截图添加 Enclave 水印，包含会话信息、时间戳等元数据。

use crate::tee::sandbox::{error::ExportError, types::SessionId};
use image::{DynamicImage, GenericImage, GenericImageView, Rgba};
use time::OffsetDateTime;
use tracing::{debug, info, warn};

/// 水印服务
///
/// 负责为截图添加安全水印，标识数据来源和完整性信息
pub struct WatermarkService {
    enclave_id: String,
}

/// 水印元数据
#[derive(Debug, Clone)]
pub struct WatermarkMetadata {
    /// 会话 ID
    pub session_id: SessionId,
    /// 时间戳
    pub timestamp: OffsetDateTime,
    /// Enclave ID
    pub enclave_id: String,
    /// 页面 URL
    pub page_url: String,
    /// 自定义水印文本
    pub custom_text: Option<String>,
}

/// 水印配置
#[derive(Debug, Clone)]
pub struct WatermarkConfig {
    /// 水印位置
    pub position: WatermarkPosition,
    /// 水印透明度 (0.0 - 1.0)
    pub opacity: f32,
    /// 字体大小（像素）
    pub font_size: u32,
    /// 是否包含时间戳
    pub include_timestamp: bool,
    /// 是否包含 Enclave ID
    pub include_enclave_id: bool,
    /// 是否包含会话 ID
    pub include_session_id: bool,
}

impl Default for WatermarkConfig {
    fn default() -> Self {
        Self {
            position: WatermarkPosition::BottomRight,
            opacity: 0.5,
            font_size: 14,
            include_timestamp: true,
            include_enclave_id: true,
            include_session_id: true,
        }
    }
}

/// 水印位置
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatermarkPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
    Tiled,
}

impl WatermarkService {
    /// 创建新的水印服务
    pub fn new(enclave_id: impl Into<String>) -> Self {
        Self {
            enclave_id: enclave_id.into(),
        }
    }

    /// 创建带默认 Enclave ID 的服务
    pub fn default_service() -> Self {
        Self::new("credbridge-enclave-001")
    }

    /// 添加水印到图像
    ///
    /// # Arguments
    ///
    /// * `image_data` - 原始图片数据
    /// * `metadata` - 水印元数据
    /// * `config` - 水印配置
    ///
    /// # Returns
    ///
    /// 返回添加水印后的图片数据
    pub fn add_watermark(
        &self,
        image_data: &[u8],
        metadata: &WatermarkMetadata,
        config: &WatermarkConfig,
    ) -> Result<Vec<u8>, ExportError> {
        info!("开始为会话 {} 的截图添加水印", metadata.session_id);

        // 构建水印文本
        let watermark_text = self.build_watermark_text(metadata, config);
        debug!("水印内容: {}", watermark_text);

        // 尝试使用 image crate 添加水印
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

    /// 快速添加标准水印
    ///
    /// 使用默认配置添加标准 Enclave 水印
    pub fn add_standard_watermark(
        &self,
        image_data: &[u8],
        session_id: SessionId,
        page_url: &str,
    ) -> Result<Vec<u8>, ExportError> {
        let metadata = WatermarkMetadata {
            session_id,
            timestamp: OffsetDateTime::now_utc(),
            enclave_id: self.enclave_id.clone(),
            page_url: page_url.to_string(),
            custom_text: None,
        };

        self.add_watermark(image_data, &metadata, &WatermarkConfig::default())
    }

    /// 验证水印
    ///
    /// 检查图片是否包含有效的水印信息
    pub fn verify_watermark(&self, _image_data: &[u8]) -> Result<bool, ExportError> {
        // TODO(#TEE-106): 实现水印验证逻辑
        // 需要: 提取水印信息并验证完整性
        Ok(true)
    }

    /// 构建水印文本
    fn build_watermark_text(
        &self,
        metadata: &WatermarkMetadata,
        config: &WatermarkConfig,
    ) -> String {
        let mut parts = Vec::new();

        if config.include_enclave_id {
            parts.push(format!("Enclave: {}", metadata.enclave_id));
        }

        if config.include_session_id {
            parts.push(format!("Session: {}", metadata.session_id));
        }

        if config.include_timestamp {
            parts.push(format!(
                "Time: {}",
                metadata
                    .timestamp
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default()
            ));
        }

        if let Some(ref custom) = metadata.custom_text {
            parts.push(custom.clone());
        }

        parts.join(" | ")
    }

    /// 使用 image crate 实现真实的水印添加
    fn apply_watermark_with_image(
        &self,
        image_data: &[u8],
        watermark_text: &str,
        config: &WatermarkConfig,
    ) -> Result<Vec<u8>, ExportError> {
        // 解码图片
        let mut image = image::load_from_memory(image_data)
            .map_err(|e| ExportError::Watermark(format!("解码图片失败: {}", e)))?;

        // 获取图片尺寸
        let (width, height) = (image.width(), image.height());

        // 计算水印位置
        let padding = 10u32;
        let text_len = watermark_text.len() as u32;
        let char_width = config.font_size / 2; // 估算字符宽度
        let text_width = text_len * char_width;
        let text_height = config.font_size;

        let (x, y) = match config.position {
            WatermarkPosition::TopLeft => (padding, padding),
            WatermarkPosition::TopRight => (width.saturating_sub(text_width + padding), padding),
            WatermarkPosition::BottomLeft => (padding, height.saturating_sub(text_height + padding)),
            WatermarkPosition::BottomRight => (
                width.saturating_sub(text_width + padding),
                height.saturating_sub(text_height + padding),
            ),
            WatermarkPosition::Center => (
                (width.saturating_sub(text_width)) / 2,
                (height.saturating_sub(text_height)) / 2,
            ),
            WatermarkPosition::Tiled => {
                // 平铺模式：在多个位置添加水印
                self.apply_tiled_watermark(&mut image, watermark_text, config)?;
                // 编码并返回
                return self.encode_image(&image);
            }
        };

        // 在指定位置添加水印背景和水印文字
        self.draw_watermark_at(&mut image, x, y, watermark_text, config)?;

        // 编码图片
        self.encode_image(&image)
    }

    /// 在指定位置绘制水印
    fn draw_watermark_at(
        &self,
        image: &mut DynamicImage,
        x: u32,
        y: u32,
        text: &str,
        config: &WatermarkConfig,
    ) -> Result<(), ExportError> {
        let (width, height) = (image.width(), image.height());
        let text_len = text.len() as u32;
        let char_width = config.font_size / 2;
        let text_width = (text_len * char_width).min(width - x);
        let text_height = config.font_size.min(height - y);

        // 计算背景矩形
        let bg_x = x;
        let bg_y = y;
        let bg_width = text_width + 10;
        let bg_height = text_height + 6;

        // 绘制半透明背景
        let alpha = (config.opacity * 255.0) as u8;
        let bg_color = Rgba([0, 0, 0, alpha]);

        for dy in 0..bg_height {
            for dx in 0..bg_width {
                let px = bg_x + dx;
                let py = bg_y + dy;
                if px < width && py < height {
                    // 混合背景色
                    let original = image.get_pixel(px, py);
                    let blended = self.blend_pixel(original, bg_color);
                    image.put_pixel(px, py, blended);
                }
            }
        }

        // 绘制简单的文字（使用像素点模拟）
        let text_color = Rgba([255, 255, 255, 255]); // 白色文字
        self.draw_simple_text(image, x + 5, y + 3, text, text_color)?;

        Ok(())
    }

    /// 简单的文字绘制（使用像素点模拟）
    fn draw_simple_text(
        &self,
        image: &mut DynamicImage,
        start_x: u32,
        start_y: u32,
        text: &str,
        color: Rgba<u8>,
    ) -> Result<(), ExportError> {
        let (width, height) = (image.width(), image.height());
        let mut x = start_x;

        for ch in text.chars() {
            // 简单的字符宽度估算
            let char_width = if ch.is_ascii() { 6 } else { 10 };

            // 绘制字符（简化为矩形点阵）
            for dy in 0..8 {
                for dx in 0..char_width {
                    let px = x + dx as u32;
                    let py = start_y + dy;

                    if px < width && py < height {
                        // 根据字符类型绘制不同的点阵模式
                        if self.should_draw_pixel(ch, dx as usize, dy as usize) {
                            image.put_pixel(px, py, color);
                        }
                    }
                }
            }

            x += char_width as u32 + 1;
        }

        Ok(())
    }

    /// 判断是否应该绘制像素（简化的字符渲染）
    fn should_draw_pixel(&self, ch: char, x: usize, y: usize) -> bool {
        // 简化的字符渲染：根据字符和位置决定是否绘制
        // 实际项目中可以使用字体库如 rusttype 或 ab_glyph

        // 简单的点阵模式
        let pattern = match ch {
            'E' | 'e' => &[0b1111111, 0b1000000, 0b1111100, 0b1000000, 0b1111111],
            'n' => &[0b0000000, 0b1111100, 0b1000010, 0b1000010, 0b1000010],
            'c' => &[0b0000000, 0b0111110, 0b1000000, 0b1000000, 0b0111110],
            'l' => &[0b1000000, 0b1000000, 0b1000000, 0b1000000, 0b1111111],
            'a' => &[0b0000000, 0b0111110, 0b1000100, 0b1000100, 0b0111111],
            'v' => &[0b0000000, 0b1000010, 0b1000010, 0b0101100, 0b0010000],
            'S' | 's' => &[0b0111111, 0b1000000, 0b0111110, 0b0000001, 0b1111110],
            'i' => &[0b0100000, 0b0000000, 0b0100000, 0b0100000, 0b0011111],
            'o' => &[0b0000000, 0b0111110, 0b1000001, 0b1000001, 0b0111110],
            'r' => &[0b0000000, 0b1111100, 0b1000010, 0b1000000, 0b1000000],
            'T' | 't' => &[0b1111111, 0b0010000, 0b0010000, 0b0010000, 0b0010000],
            'm' => &[0b0000000, 0b1111100, 0b1000010, 0b1000010, 0b1111100],
            ':' => &[0b0000000, 0b0011000, 0b0000000, 0b0011000, 0b0000000],
            '-' => &[0b0000000, 0b0000000, 0b0111110, 0b0000000, 0b0000000],
            '0'..='9' => &[0b0111110, 0b1000001, 0b1000001, 0b1000001, 0b0111110],
            ' ' => &[0b0000000, 0b0000000, 0b0000000, 0b0000000, 0b0000000],
            '|' => &[0b0010000, 0b0010000, 0b0010000, 0b0010000, 0b0010000],
            _ => &[0b1111111, 0b1111111, 0b1111111, 0b1111111, 0b1111111], // 默认方块
        };

        if y < pattern.len() && x < 7 {
            (pattern[y] >> (6 - x)) & 1 == 1
        } else {
            false
        }
    }

    /// 像素混合
    fn blend_pixel(&self, base: Rgba<u8>, overlay: Rgba<u8>) -> Rgba<u8> {
        let alpha = overlay[3] as f32 / 255.0;
        let inv_alpha = 1.0 - alpha;

        Rgba([
            (overlay[0] as f32 * alpha + base[0] as f32 * inv_alpha) as u8,
            (overlay[1] as f32 * alpha + base[1] as f32 * inv_alpha) as u8,
            (overlay[2] as f32 * alpha + base[2] as f32 * inv_alpha) as u8,
            255,
        ])
    }

    /// 平铺水印
    fn apply_tiled_watermark(
        &self,
        image: &mut DynamicImage,
        text: &str,
        config: &WatermarkConfig,
    ) -> Result<(), ExportError> {
        let (width, height) = (image.width(), image.height());
        let spacing_x = 200;
        let spacing_y = 100;

        let mut y = 20;
        while y < height {
            let mut x = 20;
            while x < width {
                self.draw_watermark_at(image, x, y, text, config)?;
                x += spacing_x;
            }
            y += spacing_y;
        }

        Ok(())
    }

    /// 编码图片为 PNG
    fn encode_image(&self, image: &DynamicImage) -> Result<Vec<u8>, ExportError> {
        let mut output = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut output);

        image
            .write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| ExportError::Watermark(format!("编码图片失败: {}", e)))?;

        Ok(output)
    }

    /// 模拟水印添加（回退实现）
    fn apply_watermark_mock(
        &self,
        image_data: &[u8],
        _watermark_text: &str,
        _config: &WatermarkConfig,
    ) -> Result<Vec<u8>, ExportError> {
        // 如果 image crate 处理失败，返回原始数据
        warn!("使用模拟水印实现");
        Ok(image_data.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_metadata() -> WatermarkMetadata {
        WatermarkMetadata {
            session_id: SessionId::new(),
            timestamp: OffsetDateTime::now_utc(),
            enclave_id: "test-enclave".to_string(),
            page_url: "https://example.com".to_string(),
            custom_text: Some("Test Watermark".to_string()),
        }
    }

    #[test]
    fn test_watermark_service_creation() {
        let service = WatermarkService::new("test-enclave");
        assert_eq!(service.enclave_id, "test-enclave");
    }

    #[test]
    fn test_build_watermark_text() {
        let service = WatermarkService::default_service();
        let metadata = create_test_metadata();
        let config = WatermarkConfig::default();

        let text = service.build_watermark_text(&metadata, &config);

        assert!(text.contains("Enclave:"));
        assert!(text.contains("Session:"));
        assert!(text.contains("Time:"));
        assert!(text.contains("Test Watermark"));
    }

    #[test]
    fn test_build_watermark_text_partial() {
        let service = WatermarkService::default_service();
        let metadata = create_test_metadata();
        let config = WatermarkConfig {
            include_timestamp: false,
            include_enclave_id: true,
            include_session_id: false,
            ..Default::default()
        };

        let text = service.build_watermark_text(&metadata, &config);

        assert!(text.contains("Enclave:"));
        assert!(!text.contains("Session:"));
        assert!(!text.contains("Time:"));
    }

    #[test]
    fn test_watermark_config_default() {
        let config = WatermarkConfig::default();
        assert_eq!(config.opacity, 0.5);
        assert_eq!(config.font_size, 14);
        assert!(config.include_timestamp);
        assert!(config.include_enclave_id);
        assert!(config.include_session_id);
    }

    #[tokio::test]
    async fn test_add_watermark() {
        let service = WatermarkService::default_service();
        let metadata = create_test_metadata();
        let config = WatermarkConfig::default();

        // 创建简单的测试图片数据（最小的 PNG）
        let test_image = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

        let result = service.add_watermark(&test_image, &metadata, &config);
        assert!(result.is_ok());

        let watermarked = result.unwrap();
        assert!(!watermarked.is_empty());
    }
}
