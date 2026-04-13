/**
 * CredBridge SDK - Token 管理模块
 *
 * 管理 bearer tokens，用于 automation token、access token 和 service account token。
 *
 * **重要说明**:
 * 此 TokenManager 管理的是对外 bearer token，
 * 包括 automation token、access token 和 service account token。
 * 浏览器侧 Privy / session 流程不属于 SDK 对外认证面。
 *
 * @module token
 */

import type { CredBridgeClient } from "./client.js";
import {
  type CreateTokenRequest,
  type CreateTokenResponse,
  type TokenInfo,
  type RequestOptions,
  type TokenMetadata,
  type TokenRevokeByIdResponse,
  type TokenStatsResponse,
  type TokenScope,
  CredBridgeError,
  CredBridgeErrorCode,
} from "./types.js";

/** Token 撤销响应 */
interface TokenRevokeResponse {
  revoked: boolean;
}

interface CreateTokenResponseApi {
  access_token: string;
  token_id: string;
  token_type: string;
  expires_in: number;
  scope: string;
  issued_at: number;
  expires_at: number;
}

interface TokenStatsResponseApi {
  total_tokens?: number;
  active_tokens: number;
  revoked_tokens?: number;
}

interface TokenMetadataApi {
  token_id: string;
  token_type: string;
  subject_type: string;
  subject_id: string;
  tenant_id: string;
  issued_from: string;
  session_id?: string | null;
  membership_id?: string | null;
  display_name?: string | null;
  granted_scopes: string[];
  expires_at: string;
  revoked_at?: string | null;
  created_at: string;
  last_used_at?: string | null;
}

function mapTokenMetadata(value: TokenMetadataApi): TokenMetadata {
  return {
    tokenId: value.token_id,
    tokenType: value.token_type,
    subjectType: value.subject_type,
    subjectId: value.subject_id,
    tenantId: value.tenant_id,
    issuedFrom: value.issued_from,
    sessionId: value.session_id ?? undefined,
    membershipId: value.membership_id ?? undefined,
    displayName: value.display_name ?? undefined,
    grantedScopes: value.granted_scopes,
    expiresAt: value.expires_at,
    revokedAt: value.revoked_at ?? undefined,
    createdAt: value.created_at,
    lastUsedAt: value.last_used_at ?? undefined,
  };
}

/**
 * Token 管理类
 *
 * 管理 bearer tokens。
 *
 * **注意**: 此类管理的 Token 用于自动化和服务集成，
 * 不负责浏览器侧 Privy / session 登录流程。
 */
export class TokenManager {
  private client: CredBridgeClient;

  constructor(client: CredBridgeClient) {
    this.client = client;
  }

  /**
   * 获取当前 Token 信息
   *
   * @returns Token 信息或 undefined
   */
  public getTokenInfo(): TokenInfo | undefined {
    return this.client.getTokenInfo();
  }

  /**
   * 获取当前 Token
   *
   * @returns Token 字符串或 undefined
   */
  public getToken(): string | undefined {
    return this.client.getToken();
  }

  /**
   * 设置新的 Token
   *
   * @param token - 新的 PASETO Token
   *
   * @example
   * ```typescript
   * sdk.token.setToken('v4.local.eyJzdWIiOiJ0ZW5hbnQxOnVzZXIxIn0...');
   * ```
   */
  public setToken(token: string): void {
    this.client.setToken(token);
  }

  /**
   * 检查 Token 是否有效
   *
   * 检查包括：
   * - Token 是否存在
   * - Token 是否已过期
   * - Token 格式是否正确
   *
   * @returns 是否有效
   *
   * @example
   * ```typescript
   * if (sdk.token.isValid()) {
   *   console.log('Token is valid');
   * }
   * ```
   */
  public isValid(): boolean {
    const tokenInfo = this.client.getTokenInfo();
    if (!tokenInfo) return false;

    const now = Math.floor(Date.now() / 1000);
    return now < tokenInfo.expiresAt;
  }

  /**
   * 检查 Token 是否即将过期
   *
   * @param bufferSeconds - 过期前缓冲时间（秒，默认 300 秒 = 5 分钟）
   * @returns 是否即将过期
   *
   * @example
   * ```typescript
   * // 检查是否将在 5 分钟内过期
   * if (sdk.token.isExpiringSoon()) {
   *   console.log('Token will expire soon');
   * }
   *
   * // 检查是否将在 10 分钟内过期
   * if (sdk.token.isExpiringSoon(600)) {
   *   console.log('Token will expire within 10 minutes');
   * }
   * ```
   */
  public isExpiringSoon(bufferSeconds = 300): boolean {
    const tokenInfo = this.client.getTokenInfo();
    if (!tokenInfo) return true;

    const now = Math.floor(Date.now() / 1000);
    return now >= tokenInfo.expiresAt - bufferSeconds;
  }

  /**
   * 获取 Token 剩余有效时间
   *
   * @returns 剩余秒数（如果 Token 无效则返回 0）
   *
   * @example
   * ```typescript
   * const remainingSeconds = sdk.token.getRemainingTime();
   * console.log(`Token expires in ${remainingSeconds} seconds`);
   * ```
   */
  public getRemainingTime(): number {
    const tokenInfo = this.client.getTokenInfo();
    if (!tokenInfo) return 0;

    const now = Math.floor(Date.now() / 1000);
    const remaining = tokenInfo.expiresAt - now;
    return Math.max(0, remaining);
  }

  /**
   * 创建新的平台 Token
   */
  public async create(
    request: CreateTokenRequest,
    options?: RequestOptions,
  ): Promise<CreateTokenResponse> {
    const response = await this.client.post<CreateTokenResponseApi>(
      "/tokens",
      {
        scopes: request.scopes,
        expires_in: request.expiresIn,
        credential_ids: request.credentialIds,
      },
      options,
    );

    return {
      accessToken: response.access_token,
      tokenId: response.token_id,
      tokenType: response.token_type,
      expiresIn: response.expires_in,
      scope: response.scope,
      issuedAt: response.issued_at,
      expiresAt: response.expires_at,
    };
  }

  /**
   * 获取 Token 统计
   */
  public async stats(options?: RequestOptions): Promise<TokenStatsResponse> {
    const response = await this.client.get<TokenStatsResponseApi>(
      "/tokens/stats",
      options,
    );
    return {
      totalTokens: response.total_tokens,
      activeTokens: response.active_tokens,
      revokedTokens: response.revoked_tokens,
    };
  }

  public async list(options?: RequestOptions): Promise<TokenMetadata[]> {
    const response = await this.client.get<TokenMetadataApi[]>(
      "/tokens",
      options,
    );
    return response.map(mapTokenMetadata);
  }

  public async get(
    tokenId: string,
    options?: RequestOptions,
  ): Promise<TokenMetadata> {
    const response = await this.client.get<TokenMetadataApi>(
      `/tokens/${tokenId}`,
      options,
    );
    return mapTokenMetadata(response);
  }

  /**
   * 撤销当前 Token
   *
   * @param options - 请求选项
   * @returns 是否撤销成功
   *
   * @deprecated 该方法保留用于兼容旧版 `/tokens/{id}/revoke` 路由。
   * 优先使用后端对齐的 `create`、`verify` 和 `stats`。
   *
   * @example
   * ```typescript
   * await sdk.token.revoke();
   * console.log('Token revoked');
   * ```
   */
  public async revoke(options?: RequestOptions): Promise<boolean> {
    const tokenInfo = this.client.getTokenInfo();
    if (!tokenInfo) {
      throw new CredBridgeError(
        CredBridgeErrorCode.InvalidToken,
        "No token to revoke",
      );
    }

    const response = await this.client.post<TokenRevokeResponse>(
      `/tokens/${tokenInfo.tokenId}/revoke`,
      {},
      options,
    );

    return response.revoked;
  }

  public async revokeById(
    tokenId: string,
    options?: RequestOptions,
  ): Promise<TokenRevokeByIdResponse> {
    const response = await this.client.post<TokenRevokeResponse>(
      `/tokens/${tokenId}/revoke`,
      {},
      options,
    );

    return {
      revoked: response.revoked,
      tokenId,
    };
  }

  /**
   * 检查 Token 是否具有指定的 Scope
   *
   * @param scope - 要检查的 Scope
   * @returns 是否具有该 Scope
   *
   * @example
   * ```typescript
   * if (sdk.token.hasScope('credential:read')) {
   *   console.log('Can read credentials');
   * }
   *
   * if (sdk.token.hasScope('credential:decrypt')) {
   *   console.log('Can decrypt credentials');
   * }
   * ```
   */
  public hasScope(scope: TokenScope | string): boolean {
    const tokenInfo = this.client.getTokenInfo();
    if (!tokenInfo) return false;

    return (
      tokenInfo.scopes.includes(scope as TokenScope) ||
      tokenInfo.scopes.includes("admin" as TokenScope)
    );
  }

  /**
   * 检查 Token 是否具有指定的任一 Scope
   *
   * @param scopes - 要检查的 Scope 列表
   * @returns 是否具有任一 Scope
   *
   * @example
   * ```typescript
   * if (sdk.token.hasAnyScope(['credential:read', 'credential:write'])) {
   *   console.log('Can read or write credentials');
   * }
   * ```
   */
  public hasAnyScope(scopes: TokenScope[] | string[]): boolean {
    return scopes.some((scope) => this.hasScope(scope));
  }

  /**
   * 检查 Token 是否具有所有指定的 Scope
   *
   * @param scopes - 要检查的 Scope 列表
   * @returns 是否具有所有 Scope
   *
   * @example
   * ```typescript
   * if (sdk.token.hasAllScopes(['credential:read', 'credential:decrypt'])) {
   *   console.log('Can read and decrypt credentials');
   * }
   * ```
   */
  public hasAllScopes(scopes: TokenScope[] | string[]): boolean {
    return scopes.every((scope) => this.hasScope(scope));
  }

  /**
   * 获取 Token 中的所有 Scope
   *
   * @returns Scope 列表
   *
   * @example
   * ```typescript
   * const scopes = sdk.token.getScopes();
   * console.log('Token scopes:', scopes);
   * ```
   */
  public getScopes(): TokenScope[] {
    const tokenInfo = this.client.getTokenInfo();
    return tokenInfo?.scopes ?? [];
  }

  /**
   * 获取租户 ID
   *
   * @returns 租户 ID 或 undefined
   */
  public getTenantId(): string | undefined {
    return this.client.getTokenInfo()?.tenantId;
  }

  /**
   * 获取用户 ID
   *
   * @returns 用户 ID 或 undefined
   */
  public getUserId(): string | undefined {
    return this.client.getTokenInfo()?.userId;
  }

  /**
   * 获取 Token ID
   *
   * @returns Token ID 或 undefined
   */
  public getTokenId(): string | undefined {
    return this.client.getTokenInfo()?.tokenId;
  }

  /**
   * 获取 Token 颁发时间
   *
   * @returns 颁发时间戳或 undefined
   */
  public getIssuedAt(): number | undefined {
    return this.client.getTokenInfo()?.issuedAt;
  }

  /**
   * 获取 Token 过期时间
   *
   * @returns 过期时间戳或 undefined
   */
  public getExpiresAt(): number | undefined {
    return this.client.getTokenInfo()?.expiresAt;
  }

  /**
   * 计算 Token 剩余有效时间的友好显示字符串
   *
   * @returns 友好格式的时间字符串（如 "5分钟", "2小时"）
   *
   * @example
   * ```typescript
   * console.log('Token expires in:', sdk.token.getRemainingTimeFormatted());
   * // 输出: "Token expires in: 5分钟"
   * ```
   */
  public getRemainingTimeFormatted(): string {
    const seconds = this.getRemainingTime();

    if (seconds === 0) {
      return "已过期";
    }

    if (seconds < 60) {
      return `${seconds}秒`;
    }

    if (seconds < 3600) {
      return `${Math.floor(seconds / 60)}分钟`;
    }

    if (seconds < 86400) {
      return `${Math.floor(seconds / 3600)}小时`;
    }

    return `${Math.floor(seconds / 86400)}天`;
  }
}
