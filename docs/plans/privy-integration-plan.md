# Privy 真实集成实施计划

**目标**: 将 CredBridge 认证系统从 Mock 模式迁移到真实 Privy 集成
**时间**: 2026-04-03
**依赖**: App ID 和 Secret 已配置到环境变量

---

## 阶段一：后端 Privy Token 验证实现 (优先级 P0)

### 任务 1.1: 添加 Privy 配置到 Rust 配置模块
**目标**: 让后端能读取 Privy 环境变量

**文件**: `src/config/mod.rs` 或新建 `src/config/privy.rs`

**需要添加的结构**:
```rust
pub struct PrivyConfig {
    pub app_id: String,
    pub app_secret: String,
    pub jwks_url: String,
    pub api_url: String,
    pub mock_enabled: bool,  // 开发回退开关
}
```

**验收标准**:
- [ ] 应用启动时能正确读取 `PRIVY_APP_ID` 等环境变量
- [ ] 提供合理的默认值用于测试
- [ ] 如果关键配置缺失，启动时给出明确错误

---

### 任务 1.2: 实现 JWKS Token 验证器
**目标**: 实现基于 JWKS 的 Privy Token 签名验证

**文件**: 新建 `src/auth/privy/jwks.rs`

**核心功能**:
1. JWKS 公钥缓存（定期刷新）
2. JWT 签名验证（使用 `jsonwebtoken` crate）
3. Token 声明解析（`did`, `wallet_address`, `email`, `custom` 等）

**依赖**: 需要添加 crate:
```toml
[dependencies]
jsonwebtoken = "9"
jwks-client-rs = "0.5"  # 或自建 JWKS 客户端
```

**验收标准**:
- [ ] 能从 Privy JWKS 端点获取公钥
- [ ] 能验证真实 Privy Token 的签名
- [ ] 能解析 Token 中的用户 DID 和钱包地址
- [ ] 有合理的错误处理（token 过期、签名无效等）

---

### 任务 1.3: 更新 `verify_privy_token` 实现
**目标**: 替换 Mock 实现为真实验证

**文件**: `src/auth/service.rs` (第 266-280 行)

**变更**:
```rust
// 当前 (Mock)
async fn verify_privy_token(&self, _token: &str) -> Result<PrivyAuthResponse, AuthError> {
    Err(AuthError::PrivyAuthenticationFailed(
        "Privy authentication not implemented".to_string(),
    ))
}

// 目标 (真实实现)
async fn verify_privy_token(&self, token: &str) -> Result<PrivyAuthResponse, AuthError> {
    if self.config.mock_enabled {
        return self.mock_verify_privy_token(token);
    }
    
    // 1. 验证 JWT 签名
    let claims = self.jwks_verifier.verify(token).await?;
    
    // 2. 解析用户信息
    Ok(PrivyAuthResponse {
        did: claims.sub,
        wallet_address: claims.custom.wallet_address,
        email: claims.custom.email,
        name: claims.custom.name,
        is_new_user: false, // 通过数据库查询判断
    })
}
```

**验收标准**:
- [ ] Mock 开关可控，便于开发调试
- [ ] 真实模式调用 JWKS 验证器
- [ ] 正确处理验证失败的各种情况

---

### 任务 1.4: 实现 `fetch_privy_mfa_status`
**目标**: 从 Privy API 获取用户 MFA 状态

**文件**: `src/auth/service.rs` (第 390-400 行)

**实现**:
```rust
async fn fetch_privy_mfa_status(&self, privy_token: &str) -> Result<MfaStatusSnapshot, AuthError> {
    let response = reqwest::Client::new()
        .get(format!("{}/users/me/mfa", self.config.privy_api_url))
        .header("Authorization", format!("Bearer {}", privy_token))
        .header("privy-app-id", &self.config.app_id)
        .send()
        .await?;
    
    let mfa_info: PrivyMfaInfo = response.json().await?;
    Ok(MfaStatusSnapshot {
        mfa_enabled: mfa_info.enabled,
        mfa_verified: mfa_info.verified,
        methods: mfa_info.methods,
    })
}
```

**验收标准**:
- [ ] 能调用 Privy API 获取 MFA 状态
- [ ] 错误时返回合理的降级行为

---

### 任务 1.5: 添加 Privy 错误类型
**目标**: 细化 Privy 相关错误

**文件**: `src/auth/error.rs`

**需要添加**:
```rust
pub enum AuthError {
    // ... 现有错误
    
    #[error("Privy token verification failed: {0}")]
    PrivyTokenVerificationFailed(String),
    
    #[error("Privy JWKS fetch failed: {0}")]
    PrivyJwksError(String),
    
    #[error("Privy API error: {status} - {message}")]
    PrivyApiError { status: u16, message: String },
    
    #[error("User MFA not satisfied")]
    MfaRequired,
}
```

**验收标准**:
- [ ] 所有 Privy 相关错误都有明确的错误类型
- [ ] 错误信息对人类友好且对调试有用

---

## 阶段二：前端 Privy SDK 集成 (优先级 P0)

### 任务 2.1: 安装 Privy React SDK
**目标**: 添加 Privy 依赖

**命令**:
```bash
cd frontend
npm install @privy-io/react-auth
```

**验收标准**:
- [ ] `package.json` 中包含 `@privy-io/react-auth`
- [ ] `package-lock.json` 已更新

---

### 任务 2.2: 配置 PrivyProvider
**目标**: 在应用根组件添加 Privy 支持

**文件**: `frontend/src/app/main.tsx` 或新建 `frontend/src/app/providers.tsx`

**实现**:
```tsx
import { PrivyProvider } from '@privy-io/react-auth';

function AppProviders({ children }: { children: React.ReactNode }) {
  return (
    <PrivyProvider
      appId={import.meta.env.VITE_PRIVY_APP_ID}
      config={{
        appearance: {
          theme: 'light',
          accentColor: '#676FFF',
          logo: zkmeLogo,
        },
        embeddedWallets: {
          createOnLogin: 'users-without-wallets',
        },
        // 支持的登录方式
        loginMethods: ['wallet', 'email', 'github', 'google'],
      }}
    >
      {children}
    </PrivyProvider>
  );
}
```

**验收标准**:
- [ ] 应用启动时 PrivyProvider 正确初始化
- [ ] 浏览器控制台无 Privy 相关错误

---

### 任务 2.3: 重构 LoginPage 使用真实 Privy
**目标**: 替换模拟 token 为真实 Privy 登录

**文件**: `frontend/src/features/auth/pages/LoginPage.tsx`

**核心变更**:
```tsx
import { usePrivy } from '@privy-io/react-auth';
import { getPrivyAccessToken } from '@/shared/auth/sessionTokens';

function LoginPage() {
  const { login, getAccessToken, authenticated, ready } = usePrivy();
  const { setPrivyState } = useAuthStore();
  const createSession = useCreateSessionAuth();
  
  const handleLogin = async () => {
    try {
      // 1. 触发 Privy 登录
      await login();
      
      // 2. 获取 Privy access token
      const privyToken = await getPrivyAccessToken(getAccessToken);
      
      // 3. 发送到后端创建 CredBridge 会话
      const result = await createSession.mutateAsync({
        privyAccessToken: privyToken,
        invitationToken: searchParams.get('invitation'),
      });
      
      // 4. 更新全局状态
      setPrivyState({
        privyAuthenticated: true,
        user: result.user,
      });
      
    } catch (error) {
      console.error('Login failed:', error);
    }
  };
  
  return (
    <PrivyLoginButton 
      onClick={handleLogin}
      isLoading={!ready || createSession.isPending}
    />
  );
}
```

**验收标准**:
- [ ] 点击登录按钮弹出 Privy 登录界面
- [ ] 成功登录后获取真实 Privy token
- [ ] Token 成功发送到后端 `/api/v1/auth/session`
- [ ] 登录成功后跳转到正确页面

---

### 任务 2.4: 更新 authStore 支持 Privy 状态
**目标**: Zustand store 支持 Privy 认证状态

**文件**: `frontend/src/shared/stores/authStore.ts`

**需要更新**:
```typescript
interface AuthState {
  privyAuthenticated: boolean;
  privyUser: PrivyUser | null;
  credbridgeSession: Session | null;
  setPrivyState: (state: Partial<AuthState>) => void;
  logout: () => Promise<void>;
}

export const useAuthStore = create<AuthState>((set, get) => ({
  privyAuthenticated: false,
  privyUser: null,
  credbridgeSession: null,
  
  logout: async () => {
    const { logout: privyLogout } = usePrivy.getState();
    await privyLogout();
    set({ privyAuthenticated: false, credbridgeSession: null });
  },
}));
```

**验收标准**:
- [ ] Store 正确跟踪 Privy 认证状态
- [ ] Logout 同时清除 Privy 和 CredBridge 会话

---

### 任务 2.5: API Client 自动注入 Privy Token
**目标**: 每次 API 调用自动携带 Privy token

**文件**: `frontend/src/shared/api/client.ts`

**实现**:
```typescript
import { getPrivyAccessToken } from '@/shared/auth/sessionTokens';

const apiClient = axios.create({
  baseURL: import.meta.env.VITE_API_URL,
});

// 请求拦截器自动添加 Privy token
apiClient.interceptors.request.use(async (config) => {
  const { getAccessToken } = usePrivy.getState();
  const token = await getPrivyAccessToken(getAccessToken);
  
  if (token) {
    config.headers.Authorization = `Bearer ${token}`;
  }
  
  return config;
});
```

**验收标准**:
- [ ] 每个 API 请求自动携带 Privy token
- [ ] Token 过期时自动刷新

---

## 阶段三：测试验证 (优先级 P0)

### 任务 3.1: 后端单元测试
**目标**: 验证 Token 验证逻辑

**文件**: `tests/auth/privy_integration_tests.rs`

**测试用例**:
- [ ] 验证有效的 Privy token 返回正确用户信息
- [ ] 验证过期 token 返回 `TokenExpired` 错误
- [ ] 验证无效签名返回 `InvalidToken` 错误
- [ ] 验证 JWKS 缓存刷新机制

---

### 任务 3.2: 端到端测试
**目标**: 完整登录流程验证

**文件**: `frontend/e2e/privy-auth.spec.ts`

**测试场景**:
- [ ] 新用户通过邀请加入租户
- [ ] 已存在用户恢复会话
- [ ] 登录后访问受保护资源
- [ ] Logout 后无法访问受保护资源

---

### 任务 3.3: 环境检查清单验证
**目标**: 对照原检查清单验证环境

**文件**: `docs/qa_reports/privy-wallet-first-user-lifecycle/env-checklist-report.md`

**验证项**:
- [ ] 所有 BLOCKER 项已解决
- [ ] 数据库连接正常
- [ ] 真实 Privy token 可验证

---

## 阶段四：部署和文档 (优先级 P1)

### 任务 4.1: 更新 API 文档
**目标**: 文档反映 Privy 认证方式

**文件**: `docs/03-API 参考/REST-API.md`

**更新内容**:
- [ ] `POST /auth/session` 请求体说明 Privy token
- [ ] 移除旧 `/auth/login` 和 `/auth/refresh` 文档
- [ ] 添加 Privy 集成 FAQ

---

### 任务 4.2: 添加部署说明
**目标**: 运维团队知道如何配置

**文件**: `docs/deployment/privy-setup.md`

**内容**:
- [ ] 环境变量清单
- [ ] 获取 Privy App ID/Secret 的步骤
- [ ] 配置 webhook (如有需要)

---

## 执行顺序建议

```
第1天：
  ├── 任务 1.1 (配置模块)
  ├── 任务 1.5 (错误类型)
  └── 任务 2.1 (安装 SDK)

第2天：
  ├── 任务 1.2 (JWKS 验证器)
  └── 任务 2.2 (PrivyProvider)

第3天：
  ├── 任务 1.3 (verify_privy_token)
  ├── 任务 1.4 (MFA 状态)
  └── 任务 2.3 (LoginPage)

第4天：
  ├── 任务 2.4 (authStore)
  ├── 任务 2.5 (API Client)
  └── 任务 3.1 (后端测试)

第5天：
  ├── 任务 3.2 (E2E 测试)
  ├── 任务 3.3 (环境验证)
  └── 任务 4.1/4.2 (文档)
```

---

## 风险缓解

| 风险 | 缓解措施 |
|------|----------|
| Privy API 不稳定 | 保留 MOCK_PRIVY_AUTH 开关，可快速回退 |
| JWKS 请求失败 | 实现本地缓存和重试机制 |
| 前端 SDK 加载慢 | 使用动态导入，添加加载状态 |
| Token 过期处理 | 自动刷新机制，用户无感知 |

---

## 验收标准 (Definition of Done)

- [ ] 用户可以通过 Privy 钱包登录
- [ ] 登录后后端能正确验证 Privy token
- [ ] 能创建 CredBridge 会话并返回用户/租户信息
- [ ] 旧 `/auth/refresh` 链路不再被调用
- [ ] 所有 P0 claim 测试通过
- [ ] 文档已更新
- [ ] Code Review 通过
