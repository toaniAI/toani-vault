/**
 * CredBridge SDK - 凭证管理服务
 *
 * 提供凭证的 CRUD 操作和解密功能
 */

import type { CredBridgeClient } from "./client.js";
import {
  type CreateCredentialRequest,
  type CreateCredentialResponse,
  type CredentialMetadata,
  type GetCredentialResponse,
  type ListCredentialsResponse,
  type DecryptCredentialRequest,
  type DecryptCredentialResponse,
  type DeleteCredentialResponse,
  type CredentialFilter,
  CredentialType,
  type RequestOptions,
} from "./types.js";

/**
 * 凭证管理服务
 */
export class CredentialsService {
  private client: CredBridgeClient;

  constructor(client: CredBridgeClient) {
    this.client = client;
  }

  /**
   * 创建新凭证
   *
   * @param request - 创建凭证请求
   * @param options - 请求选项
   * @returns 创建的凭证信息
   *
   * @example
   * ```typescript
   * const credential = await sdk.credentials.create({
   *   serviceId: 'schwab',
   *   credentialType: CredentialType.UsernamePassword,
   *   plaintextData: {
   *     username: 'user@example.com',
   *     password: 'secret_password'
   *   },
   *   expiresAt: Math.floor(Date.now() / 1000) + 86400 * 30 // 30天后过期
   * });
   * console.log('Created credential:', credential.credentialId);
   * ```
   */
  public async create(
    request: CreateCredentialRequest,
    options?: RequestOptions,
  ): Promise<CreateCredentialResponse> {
    return this.client.post<CreateCredentialResponse>(
      "/credentials",
      {
        service_id: request.serviceId,
        credential_type: request.credentialType,
        plaintext_data: request.plaintextData,
        expires_at: request.expiresAt,
      },
      options,
    );
  }

  /**
   * 创建用户名密码凭证（快捷方法）
   *
   * @param serviceId - 服务 ID
   * @param username - 用户名
   * @param password - 密码
   * @param options - 可选参数（过期时间、请求选项等）
   * @returns 创建的凭证信息
   *
   * @example
   * ```typescript
   * const credential = await sdk.credentials.createUsernamePassword(
   *   'schwab',
   *   'user@example.com',
   *   'secret_password'
   * );
   * ```
   */
  public async createUsernamePassword(
    serviceId: string,
    username: string,
    password: string,
    options?: {
      expiresAt?: number;
      requestOptions?: RequestOptions;
    },
  ): Promise<CreateCredentialResponse> {
    return this.create(
      {
        serviceId,
        credentialType: CredentialType.UsernamePassword,
        plaintextData: { username, password },
        expiresAt: options?.expiresAt,
      },
      options?.requestOptions,
    );
  }

  /**
   * 创建 API Key 凭证（快捷方法）
   *
   * @param serviceId - 服务 ID
   * @param apiKey - API Key
   * @param apiSecret - API Secret（可选）
   * @param options - 可选参数
   * @returns 创建的凭证信息
   *
   * @example
   * ```typescript
   * const credential = await sdk.credentials.createApiKey(
   *   'stripe',
   *   'sk_live_...',
   *   undefined,
   *   { expiresAt: Math.floor(Date.now() / 1000) + 86400 * 90 }
   * );
   * ```
   */
  public async createApiKey(
    serviceId: string,
    apiKey: string,
    apiSecret?: string,
    options?: {
      expiresAt?: number;
      requestOptions?: RequestOptions;
    },
  ): Promise<CreateCredentialResponse> {
    const plaintextData: Record<string, string> = { api_key: apiKey };
    if (apiSecret) {
      plaintextData.api_secret = apiSecret;
    }

    return this.create(
      {
        serviceId,
        credentialType: CredentialType.ApiKey,
        plaintextData,
        expiresAt: options?.expiresAt,
      },
      options?.requestOptions,
    );
  }

  /**
   * 创建 OAuth 刷新令牌凭证（快捷方法）
   *
   * @param serviceId - 服务 ID
   * @param refreshToken - 刷新令牌
   * @param options - 可选参数
   * @returns 创建的凭证信息
   *
   * @example
   * ```typescript
   * const credential = await sdk.credentials.createOAuthRefresh(
   *   'google',
   *   '1//0d...'
   * );
   * ```
   */
  public async createOAuthRefresh(
    serviceId: string,
    refreshToken: string,
    options?: {
      expiresAt?: number;
      requestOptions?: RequestOptions;
    },
  ): Promise<CreateCredentialResponse> {
    return this.create(
      {
        serviceId,
        credentialType: CredentialType.OAuthRefresh,
        plaintextData: { refreshToken },
        expiresAt: options?.expiresAt,
      },
      options?.requestOptions,
    );
  }

  /**
   * 获取凭证列表
   *
   * @param filter - 过滤条件
   * @param options - 请求选项
   * @returns 凭证列表
   *
   * @example
   * ```typescript
   * // 获取所有凭证
   * const { credentials, total } = await sdk.credentials.list();
   *
   * // 按服务 ID 过滤
   * const { credentials } = await sdk.credentials.list({
   *   serviceId: 'schwab'
   * });
   *
   * // 按凭证类型过滤
   * const { credentials } = await sdk.credentials.list({
   *   credentialType: CredentialType.ApiKey
   * });
   * ```
   */
  public async list(
    filter?: CredentialFilter,
    options?: RequestOptions,
  ): Promise<{ credentials: CredentialMetadata[]; total: number }> {
    // 构建查询参数
    const queryParams = new URLSearchParams();
    if (filter?.serviceId) {
      queryParams.append("service_id", filter.serviceId);
    }
    if (filter?.credentialType) {
      queryParams.append("credential_type", filter.credentialType);
    }
    if (filter?.includeDeleted !== undefined) {
      queryParams.append("include_deleted", String(filter.includeDeleted));
    }
    if (filter?.onlyValid !== undefined) {
      queryParams.append("only_valid", String(filter.onlyValid));
    }

    const queryString = queryParams.toString();
    const path = queryString ? `/credentials?${queryString}` : "/credentials";

    const response = await this.client.get<ListCredentialsResponse>(
      path,
      options,
    );
    return { credentials: response.credentials, total: response.total };
  }

  /**
   * 获取单个凭证详情
   *
   * @param credentialId - 凭证 ID
   * @param options - 请求选项
   * @returns 凭证详情
   *
   * @example
   * ```typescript
   * const credential = await sdk.credentials.get('018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c');
   * console.log('Service:', credential.serviceId);
   * console.log('Type:', credential.credentialType);
   * ```
   */
  public async get(
    credentialId: string,
    options?: RequestOptions,
  ): Promise<GetCredentialResponse> {
    return this.client.get<GetCredentialResponse>(
      `/credentials/${credentialId}`,
      options,
    );
  }

  /**
   * 解密凭证
   *
   * @param credentialId - 凭证 ID
   * @param reason - 解密理由（用于审计）
   * @param options - 请求选项
   * @returns 解密的凭证数据
   *
   * @example
   * ```typescript
   * const decrypted = await sdk.credentials.decrypt(
   *   '018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c',
   *   '用户登录操作'
   * );
   * console.log('Username:', decrypted.plaintextData.username);
   * console.log('Password:', decrypted.plaintextData.password);
   * ```
   */
  public async decrypt(
    credentialId: string,
    reason?: string,
    options?: RequestOptions,
  ): Promise<DecryptCredentialResponse> {
    const request: DecryptCredentialRequest = { reason };
    return this.client.post<DecryptCredentialResponse>(
      `/credentials/${credentialId}/decrypt`,
      request,
      options,
    );
  }

  /**
   * 删除凭证
   *
   * @param credentialId - 凭证 ID
   * @param options - 请求选项
   * @returns 删除结果
   *
   * @example
   * ```typescript
   * const result = await sdk.credentials.delete('018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c');
   * if (result.deleted) {
   *   console.log('Credential deleted successfully');
   * }
   * ```
   */
  public async delete(
    credentialId: string,
    options?: RequestOptions,
  ): Promise<DeleteCredentialResponse> {
    return this.client.delete<DeleteCredentialResponse>(
      `/credentials/${credentialId}`,
      options,
    );
  }

  /**
   * 获取指定服务的所有凭证
   *
   * @param serviceId - 服务 ID
   * @param options - 请求选项
   * @returns 凭证列表
   *
   * @example
   * ```typescript
   * const { credentials } = await sdk.credentials.getByService('schwab');
   * ```
   */
  public async getByService(
    serviceId: string,
    options?: RequestOptions,
  ): Promise<{ credentials: CredentialMetadata[]; total: number }> {
    return this.list({ serviceId }, options);
  }

  /**
   * 获取指定类型的所有凭证
   *
   * @param credentialType - 凭证类型
   * @param options - 请求选项
   * @returns 凭证列表
   *
   * @example
   * ```typescript
   * const { credentials } = await sdk.credentials.getByType(CredentialType.ApiKey);
   * ```
   */
  public async getByType(
    credentialType: CredentialType,
    options?: RequestOptions,
  ): Promise<{ credentials: CredentialMetadata[]; total: number }> {
    return this.list({ credentialType }, options);
  }

  /**
   * 检查凭证是否存在
   *
   * @param credentialId - 凭证 ID
   * @param options - 请求选项
   * @returns 是否存在
   *
   * @example
   * ```typescript
   * const exists = await sdk.credentials.exists('018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c');
   * console.log('Exists:', exists);
   * ```
   */
  public async exists(
    credentialId: string,
    options?: RequestOptions,
  ): Promise<boolean> {
    try {
      await this.get(credentialId, { ...options, skipRetry: true });
      return true;
    } catch (error) {
      return false;
    }
  }
}
