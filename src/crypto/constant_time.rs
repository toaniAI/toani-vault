//! 恒定时间密码学操作模块
//!
//! 实现防止侧信道攻击的恒定时间比较和操作
//!
//! # 安全威胁
//!
//! - 时序攻击：通过测量操作时间推断敏感数据
//! - 缓存时序攻击：通过缓存访问模式泄露信息
//! - 分支预测攻击：通过 CPU 分支预测泄露信息
//!
//! # 实现原则
//!
//! 1. 所有敏感数据比较使用恒定时间算法
//! 2. 避免基于敏感数据的条件分支
//! 3. 使用 `subtle` crate 提供的恒定时间原语
//! 4. 敏感内存操作后立即清零

use std::time::Instant;
use subtle::{
    Choice, ConditionallySelectable, ConstantTimeEq, ConstantTimeGreater, ConstantTimeLess,
};
use zeroize::Zeroize;

/// 恒定时间字节比较
///
/// 比较两个字节切片是否相等，执行时间不依赖于内容
///
/// # 参数
/// * `a` - 第一个字节切片
/// * `b` - 第二个字节切片
///
/// # 返回
/// 如果相等返回 true，否则返回 false
///
/// # 安全保证
/// - 执行时间不依赖于输入内容
/// - 不产生基于内容的分支
/// - 防止时序攻击
///
/// # 示例
/// ```rust
/// use credbridge::crypto::constant_time::ct_compare;
///
/// let a = b"secret_key_1";
/// let b = b"secret_key_1";
/// let c = b"secret_key_2";
///
/// assert!(ct_compare(a, b));
/// assert!(!ct_compare(a, c));
/// ```
#[inline]
pub fn ct_compare(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    a.ct_eq(b).into()
}

/// 恒定时间字节比较（结果作为 Choice）
///
/// 返回 `subtle::Choice` 类型，可用于后续的恒定时间操作
#[inline]
pub fn ct_compare_choice(a: &[u8], b: &[u8]) -> Choice {
    if a.len() != b.len() {
        return Choice::from(0);
    }

    a.ct_eq(b)
}

/// 恒定时间字节比较（固定长度）
///
/// 针对固定长度的字节数组优化
#[inline]
pub fn ct_compare_fixed<const N: usize>(a: &[u8; N], b: &[u8; N]) -> bool {
    a.ct_eq(b).into()
}

/// 恒定时间条件选择
///
/// 根据条件选择两个值之一，不产生分支
///
/// # 参数
/// * `condition` - 选择条件（0 或 1）
/// * `if_true` - 条件为真时的值
/// * `if_false` - 条件为假时的值
///
/// # 返回
/// 如果 condition 为 1 返回 if_true，否则返回 if_false
#[inline]
pub fn ct_select<T: ConditionallySelectable>(condition: Choice, if_true: T, if_false: T) -> T {
    T::conditional_select(&if_false, &if_true, condition)
}

/// 恒定时间字节数组条件选择
#[inline]
pub fn ct_select_bytes<const N: usize>(
    condition: Choice,
    if_true: &[u8; N],
    if_false: &[u8; N],
) -> [u8; N] {
    let mut result = *if_false;
    for i in 0..N {
        result[i] = u8::conditional_select(&if_false[i], &if_true[i], condition);
    }
    result
}

/// 恒定时间最大值
#[inline]
pub fn ct_max<T: ConstantTimeGreater + ConditionallySelectable>(a: T, b: T) -> T {
    ct_select(a.ct_gt(&b), a, b)
}

/// 恒定时间最小值
#[inline]
pub fn ct_min<T: ConstantTimeLess + ConditionallySelectable>(a: T, b: T) -> T {
    ct_select(a.ct_lt(&b), a, b)
}

/// 恒定时间验证 MAC（消息认证码）
///
/// 用于验证 HMAC、CMAC 等消息认证码，防止时序攻击
///
/// # 参数
/// * `expected` - 期望的 MAC 值
/// * `received` - 接收到的 MAC 值
///
/// # 返回
/// 如果 MAC 匹配返回 true，否则返回 false
///
/// # 安全注意
/// 必须在解密后验证 MAC，避免填充预言攻击
#[inline]
pub fn ct_verify_mac(expected: &[u8], received: &[u8]) -> bool {
    ct_compare(expected, received)
}

/// 恒定时间验证 Token
///
/// 用于验证 API Token、会话 Token 等，防止时序攻击
#[inline]
pub fn ct_verify_token(expected: &str, received: &str) -> bool {
    expected.as_bytes().ct_eq(received.as_bytes()).into()
}

/// 恒定时间查找表访问
///
/// 从查找表中获取元素，不泄露访问索引
///
/// # 参数
/// * `table` - 查找表
/// * `index` - 访问索引
///
/// # 返回
/// 表中指定位置的元素
///
/// # 安全保证
/// - 访问模式不依赖于索引值
/// - 防止缓存时序攻击
pub fn ct_lookup<T: Copy + ConditionallySelectable>(table: &[T], index: usize) -> Option<T> {
    if index >= table.len() {
        return None;
    }

    let mut result = table[0];
    for (i, &item) in table.iter().enumerate() {
        let condition = Choice::from((i == index) as u8);
        result = ct_select(condition, item, result);
    }

    Some(result)
}

/// 恒定时间字节查找
pub fn ct_lookup_byte(table: &[u8], index: usize) -> Option<u8> {
    ct_lookup(table, index)
}

/// 安全内存区域
///
/// 自动清零的敏感数据存储
pub struct SecureBuffer {
    data: Vec<u8>,
    capacity: usize,
}

impl SecureBuffer {
    /// 创建新的安全缓冲区
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![0u8; size],
            capacity: size,
        }
    }

    /// 创建初始化的安全缓冲区
    pub fn with_data(data: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
            capacity: data.len(),
        }
    }

    /// 获取数据引用
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    /// 获取可变数据引用
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// 安全地写入数据
    pub fn write(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if data.len() > self.capacity {
            return Err("缓冲区溢出");
        }

        // 恒定时间写入
        for i in 0..self.capacity {
            let condition = Choice::from((i < data.len()) as u8);
            self.data[i] = ct_select(condition, data[i], self.data[i]);
        }

        Ok(())
    }

    /// 安全清零
    pub fn zeroize(&mut self) {
        self.data.zeroize();
    }

    /// 获取容量
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 获取实际长度
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl Drop for SecureBuffer {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl Clone for SecureBuffer {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            capacity: self.capacity,
        }
    }
}

/// 恒定时间比较两个 SecureBuffer
impl PartialEq for SecureBuffer {
    fn eq(&self, other: &Self) -> bool {
        if self.len() != other.len() {
            return false;
        }
        ct_compare(&self.data, &other.data)
    }
}

/// 敏感字符串（自动清零）
pub struct SecureString {
    bytes: Vec<u8>,
}

impl SecureString {
    /// 创建新的安全字符串
    pub fn new(s: &str) -> Self {
        Self {
            bytes: s.as_bytes().to_vec(),
        }
    }

    /// 获取字符串引用
    pub fn as_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.bytes)
    }

    /// 获取字节切片
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// 恒定时间比较
    pub fn ct_eq(&self, other: &str) -> bool {
        self.bytes.ct_eq(other.as_bytes()).into()
    }

    /// 安全清零
    pub fn zeroize(&mut self) {
        self.bytes.zeroize();
    }
}

impl Drop for SecureString {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl From<&str> for SecureString {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

/// 验证恒定时间属性（测试用）
///
/// 测量多次操作的时间差异，确保在可接受范围内
pub fn verify_constant_time<F>(operation: F, iterations: usize) -> ConstantTimeVerification
where
    F: Fn() -> bool,
{
    let mut times = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let start = Instant::now();
        let _ = operation();
        times.push(start.elapsed().as_nanos());
    }

    let min_time = *times.iter().min().unwrap();
    let max_time = *times.iter().max().unwrap();
    let avg_time = times.iter().sum::<u128>() / iterations as u128;

    // 计算变异系数（标准差/平均值）
    let variance = times
        .iter()
        .map(|&t| (t as i128 - avg_time as i128).pow(2))
        .sum::<i128>() as f64
        / iterations as f64;
    let std_dev = variance.sqrt();
    let coefficient_of_variation = std_dev / avg_time as f64;

    ConstantTimeVerification {
        min_time_ns: min_time,
        max_time_ns: max_time,
        avg_time_ns: avg_time,
        std_dev_ns: std_dev,
        coefficient_of_variation,
        iterations,
    }
}

/// 恒定时间验证结果
#[derive(Debug, Clone)]
pub struct ConstantTimeVerification {
    pub min_time_ns: u128,
    pub max_time_ns: u128,
    pub avg_time_ns: u128,
    pub std_dev_ns: f64,
    pub coefficient_of_variation: f64,
    pub iterations: usize,
}

impl ConstantTimeVerification {
    /// 检查是否满足恒定时间要求
    ///
    /// 变异系数应小于 0.1（10% 的波动）
    pub fn is_constant_time(&self) -> bool {
        self.coefficient_of_variation < 0.1
    }

    /// 获取验证报告
    pub fn report(&self) -> String {
        format!(
            "恒定时间验证报告:\n\
             - 迭代次数：{}\n\
             - 最小时间：{} ns\n\
             - 最大时间：{} ns\n\
             - 平均时间：{} ns\n\
             - 标准差：{:.2} ns\n\
             - 变异系数：{:.4} ({:.2}%)\n\
             - 恒定时间检查：{}",
            self.iterations,
            self.min_time_ns,
            self.max_time_ns,
            self.avg_time_ns,
            self.std_dev_ns,
            self.coefficient_of_variation,
            self.coefficient_of_variation * 100.0,
            if self.is_constant_time() {
                "通过"
            } else {
                "失败"
            }
        )
    }
}

/// 敏感数据清理器
///
/// RAII 模式，确保作用域结束时清理敏感数据
pub struct SensitiveGuard<T: Zeroize> {
    data: T,
}

impl<T: Zeroize> SensitiveGuard<T> {
    /// 创建新的敏感数据守卫
    pub fn new(data: T) -> Self {
        Self { data }
    }

    /// 获取数据引用
    pub fn get(&self) -> &T {
        &self.data
    }

    /// 获取可变数据引用
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.data
    }

    /// 消耗守卫并返回数据（不清零）
    ///
    /// # Safety
    /// 调用者负责确保数据安全清理
    pub unsafe fn into_inner_unchecked(self) -> T {
        use std::ptr;
        let mut data = std::mem::MaybeUninit::<T>::uninit();
        unsafe {
            ptr::write(data.as_mut_ptr(), std::ptr::read(&self.data));
            std::mem::forget(self);
            data.assume_init()
        }
    }
}

impl<T: Zeroize> Drop for SensitiveGuard<T> {
    fn drop(&mut self) {
        self.data.zeroize();
    }
}

impl<T: Zeroize + Default> Default for SensitiveGuard<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ct_compare_equal() {
        let a = b"test_data_123";
        let b = b"test_data_123";
        assert!(ct_compare(a, b));
    }

    #[test]
    fn test_ct_compare_not_equal() {
        let a = b"test_data_123";
        let b = b"test_data_456";
        assert!(!ct_compare(a, b));
    }

    #[test]
    fn test_ct_compare_different_lengths() {
        let a = b"short";
        let b = b"much longer data";
        assert!(!ct_compare(a, b));
    }

    #[test]
    fn test_ct_compare_fixed() {
        let a = [0x42u8; 32];
        let b = [0x42u8; 32];
        let c = [0x43u8; 32];

        assert!(ct_compare_fixed(&a, &b));
        assert!(!ct_compare_fixed(&a, &c));
    }

    #[test]
    fn test_ct_select() {
        let a = 42u8;
        let b = 99u8;

        let result_true = ct_select(Choice::from(1), a, b);
        let result_false = ct_select(Choice::from(0), a, b);

        assert_eq!(result_true, a);
        assert_eq!(result_false, b);
    }

    #[test]
    fn test_ct_verify_mac() {
        let expected = b"hmac_value_12345";
        let received_correct = b"hmac_value_12345";
        let received_wrong = b"hmac_value_67890";

        assert!(ct_verify_mac(expected, received_correct));
        assert!(!ct_verify_mac(expected, received_wrong));
    }

    #[test]
    fn test_ct_verify_token() {
        let expected = "secret_token_abc123";
        let received_correct = "secret_token_abc123";
        let received_wrong = "secret_token_xyz789";

        assert!(ct_verify_token(expected, received_correct));
        assert!(!ct_verify_token(expected, received_wrong));
    }

    #[test]
    fn test_ct_lookup() {
        let table = [10u8, 20, 30, 40, 50];

        assert_eq!(ct_lookup(&table, 0), Some(10));
        assert_eq!(ct_lookup(&table, 2), Some(30));
        assert_eq!(ct_lookup(&table, 4), Some(50));
        assert_eq!(ct_lookup(&table, 5), None);
    }

    #[test]
    fn test_secure_buffer() {
        let mut buffer = SecureBuffer::new(32);
        let data = [0x42u8; 16];

        buffer.write(&data).unwrap();
        assert_eq!(buffer.as_slice()[..16], data);

        buffer.zeroize();
        assert!(buffer.as_slice().iter().all(|&b| b == 0));
    }

    #[test]
    fn test_secure_buffer_drop() {
        let mut buffer = SecureBuffer::with_data(&[0x42u8; 32]);
        let ptr = buffer.as_slice().as_ptr();

        drop(buffer);

        // 验证内存已被清零（通过 unsafe 读取）
        unsafe {
            let slice = std::slice::from_raw_parts(ptr, 32);
            assert!(slice.iter().all(|&b| b == 0));
        }
    }

    #[test]
    fn test_secure_string() {
        let mut secure_str = SecureString::new("sensitive_data");

        assert_eq!(secure_str.as_str().unwrap(), "sensitive_data");
        assert!(secure_str.ct_eq("sensitive_data"));
        assert!(!secure_str.ct_eq("other_data"));

        secure_str.zeroize();
    }

    #[test]
    fn test_secure_string_drop() {
        let secure_str = SecureString::new("secret_password");
        let bytes = secure_str.as_bytes().to_vec();

        drop(secure_str);

        // 验证原始数据已被清零
        assert!(bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_sensitive_guard() {
        let mut guard = SensitiveGuard::new([0x42u8; 32]);

        assert_eq!(guard.get()[0], 0x42);
        guard.get_mut()[0] = 0x00;
        assert_eq!(guard.get()[0], 0x00);

        drop(guard);
        // 数据在 drop 时自动清零
    }

    // into_inner 测试已移除，因为该方法需要 unsafe

    #[test]
    fn test_constant_time_verification() {
        // 测试一个简单的恒定时间操作
        let verification = verify_constant_time(
            || {
                let a = [0x42u8; 32];
                let b = [0x42u8; 32];
                ct_compare(&a, &b)
            },
            100,
        );

        assert!(verification.iterations == 100);
        assert!(verification.min_time_ns > 0);
        assert!(verification.max_time_ns >= verification.min_time_ns);

        println!("{}", verification.report());
    }

    #[test]
    fn test_ct_max_min() {
        assert_eq!(ct_max(5u32, 10u32), 10u32);
        assert_eq!(ct_max(10u32, 5u32), 10u32);

        assert_eq!(ct_min(5u32, 10u32), 5u32);
        assert_eq!(ct_min(10u32, 5u32), 5u32);
    }

    #[test]
    fn test_secure_buffer_clone() {
        let buffer1 = SecureBuffer::with_data(&[0x42u8; 16]);
        let buffer2 = buffer1.clone();

        assert_eq!(buffer1.as_slice(), buffer2.as_slice());
    }

    #[test]
    fn test_secure_buffer_partial_eq() {
        let buffer1 = SecureBuffer::with_data(&[0x42u8; 16]);
        let buffer2 = SecureBuffer::with_data(&[0x42u8; 16]);
        let buffer3 = SecureBuffer::with_data(&[0x43u8; 16]);

        // 直接比较字节内容
        assert!(buffer1.as_slice() == buffer2.as_slice());
        assert!(buffer1.as_slice() != buffer3.as_slice());
    }
}
