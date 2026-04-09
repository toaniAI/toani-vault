/**
 * CredBridge SDK Token 管理测试
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { TokenManager } from '../src/token.js';
import { CredBridgeClient } from '../src/client.js';
import {
  TokenScope,
  CredBridgeError,
  CredBridgeErrorCode,
} from '../src/types.js';

describe('TokenManager', () => {
  const mockBaseUrl = 'https://vault.credbridge.io';

  // 创建一个有效的 Token payload（base64url 编码）
  const createMockToken = (claims: Record<string, unknown>): string => {
    const payload = Buffer.from(JSON.stringify(claims)).toString('base64url');
    return `v4.local.${payload}.signature`;
  };

  let client: CredBridgeClient;
  let tokenManager: TokenManager;

  beforeEach(() => {
    vi.resetAllMocks();
  });

  describe('Token 信息获取', () => {
    it('应该返回 Token 信息', () => {
      const token = createMockToken({
        sub: 'tenant1:user1',
        exp: Math.floor(Date.now() / 1000) + 3600,
        iat: Math.floor(Date.now() / 1000),
        jti: 'token123',
        scope: 'credential:read credential:write',
        tenant_id: 'tenant1',
      });

      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
      });
      tokenManager = new TokenManager(client);

      const info = tokenManager.getTokenInfo();
      expect(info).toBeDefined();
      expect(info?.tokenId).toBe('token123');
      expect(info?.tenantId).toBe('tenant1');
      expect(info?.userId).toBe('user1');
    });

    it('应该在无 Token 时返回 undefined', () => {
      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
      });
      tokenManager = new TokenManager(client);

      expect(tokenManager.getTokenInfo()).toBeUndefined();
      expect(tokenManager.getToken()).toBeUndefined();
    });

    it('应该能够获取原始 Token', () => {
      const token = createMockToken({
        sub: 'tenant1:user1',
        exp: Math.floor(Date.now() / 1000) + 3600,
        iat: Math.floor(Date.now() / 1000),
        jti: 'token123',
        scope: 'credential:read',
      });

      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
      });
      tokenManager = new TokenManager(client);

      expect(tokenManager.getToken()).toBe(token);
    });
  });

  describe('Token 有效性检查', () => {
    it('应该在 Token 有效时返回 true', () => {
      const token = createMockToken({
        sub: 'tenant1:user1',
        exp: Math.floor(Date.now() / 1000) + 3600, // 1小时后过期
        iat: Math.floor(Date.now() / 1000),
        jti: 'token123',
        scope: 'credential:read',
      });

      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
      });
      tokenManager = new TokenManager(client);

      expect(tokenManager.isValid()).toBe(true);
    });

    it('应该在 Token 过期时返回 false', () => {
      const token = createMockToken({
        sub: 'tenant1:user1',
        exp: Math.floor(Date.now() / 1000) - 3600, // 1小时前过期
        iat: Math.floor(Date.now() / 1000) - 7200,
        jti: 'token123',
        scope: 'credential:read',
      });

      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
      });
      tokenManager = new TokenManager(client);

      expect(tokenManager.isValid()).toBe(false);
    });

    it('应该在无 Token 时返回 false', () => {
      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
      });
      tokenManager = new TokenManager(client);

      expect(tokenManager.isValid()).toBe(false);
    });
  });

  describe('Token 即将过期检查', () => {
    it('应该在 Token 即将过期时返回 true', () => {
      const token = createMockToken({
        sub: 'tenant1:user1',
        exp: Math.floor(Date.now() / 1000) + 180, // 3分钟后过期
        iat: Math.floor(Date.now() / 1000),
        jti: 'token123',
        scope: 'credential:read',
      });

      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
      });
      tokenManager = new TokenManager(client);

      // 使用 5 分钟缓冲期
      expect(tokenManager.isExpiringSoon(300)).toBe(true);
    });

    it('应该在 Token 还有较长时间时返回 false', () => {
      const token = createMockToken({
        sub: 'tenant1:user1',
        exp: Math.floor(Date.now() / 1000) + 3600, // 1小时后过期
        iat: Math.floor(Date.now() / 1000),
        jti: 'token123',
        scope: 'credential:read',
      });

      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
      });
      tokenManager = new TokenManager(client);

      expect(tokenManager.isExpiringSoon(300)).toBe(false);
    });
  });

  describe('剩余时间计算', () => {
    it('应该正确计算剩余时间', () => {
      const exp = Math.floor(Date.now() / 1000) + 3600; // 1小时后过期
      const token = createMockToken({
        sub: 'tenant1:user1',
        exp,
        iat: Math.floor(Date.now() / 1000),
        jti: 'token123',
        scope: 'credential:read',
      });

      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
      });
      tokenManager = new TokenManager(client);

      const remaining = tokenManager.getRemainingTime();
      // 允许 5 秒的误差
      expect(remaining).toBeGreaterThan(3590);
      expect(remaining).toBeLessThanOrEqual(3600);
    });

    it('应该在 Token 过期时返回 0', () => {
      const token = createMockToken({
        sub: 'tenant1:user1',
        exp: Math.floor(Date.now() / 1000) - 3600, // 1小时前过期
        iat: Math.floor(Date.now() / 1000) - 7200,
        jti: 'token123',
        scope: 'credential:read',
      });

      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
      });
      tokenManager = new TokenManager(client);

      expect(tokenManager.getRemainingTime()).toBe(0);
    });

    it('应该格式化剩余时间', () => {
      const testCases = [
        { seconds: 30, expected: '30秒' },
        { seconds: 300, expected: '5分钟' },
        { seconds: 3600, expected: '1小时' },
        { seconds: 86400, expected: '1天' },
        { seconds: 0, expected: '已过期' },
      ];

      for (const { seconds, expected } of testCases) {
        const token = createMockToken({
          sub: 'tenant1:user1',
          exp: Math.floor(Date.now() / 1000) + seconds,
          iat: Math.floor(Date.now() / 1000),
          jti: 'token123',
          scope: 'credential:read',
        });

        client = new CredBridgeClient({
          baseUrl: mockBaseUrl,
          token,
        });
        tokenManager = new TokenManager(client);

        expect(tokenManager.getRemainingTimeFormatted()).toBe(expected);
      }
    });
  });

  describe('Scope 检查', () => {
    const createClientWithScopes = (scopes: string[]) => {
      const token = createMockToken({
        sub: 'tenant1:user1',
        exp: Math.floor(Date.now() / 1000) + 3600,
        iat: Math.floor(Date.now() / 1000),
        jti: 'token123',
        scope: scopes.join(' '),
      });

      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
      });
      return new TokenManager(client);
    };

    it('hasScope 应该正确判断', () => {
      tokenManager = createClientWithScopes(['credential:read']);

      expect(tokenManager.hasScope(TokenScope.CredentialRead)).toBe(true);
      expect(tokenManager.hasScope(TokenScope.CredentialWrite)).toBe(false);
    });

    it('admin scope 应该拥有所有权限', () => {
      tokenManager = createClientWithScopes(['admin']);

      expect(tokenManager.hasScope(TokenScope.CredentialRead)).toBe(true);
      expect(tokenManager.hasScope(TokenScope.CredentialWrite)).toBe(true);
      expect(tokenManager.hasScope(TokenScope.CredentialDecrypt)).toBe(true);
      expect(tokenManager.hasScope(TokenScope.AuditRead)).toBe(true);
    });

    it('hasAnyScope 应该正确判断', () => {
      tokenManager = createClientWithScopes(['credential:read']);

      expect(tokenManager.hasAnyScope([TokenScope.CredentialRead, TokenScope.CredentialWrite])).toBe(true);
      expect(tokenManager.hasAnyScope([TokenScope.CredentialWrite, TokenScope.CredentialDecrypt])).toBe(false);
    });

    it('hasAllScopes 应该正确判断', () => {
      tokenManager = createClientWithScopes(['credential:read', 'credential:write']);

      expect(tokenManager.hasAllScopes([TokenScope.CredentialRead])).toBe(true);
      expect(tokenManager.hasAllScopes([TokenScope.CredentialRead, TokenScope.CredentialWrite])).toBe(true);
      expect(tokenManager.hasAllScopes([TokenScope.CredentialRead, TokenScope.CredentialDecrypt])).toBe(false);
    });

    it('应该返回所有 scopes', () => {
      tokenManager = createClientWithScopes(['credential:read', 'credential:write']);

      const scopes = tokenManager.getScopes();
      expect(scopes).toContain('credential:read');
      expect(scopes).toContain('credential:write');
      expect(scopes).toHaveLength(2);
    });
  });

  describe('Token 验证', () => {
    it('应该成功验证有效 Token', async () => {
      const token = createMockToken({
        sub: 'tenant1:user1',
        exp: Math.floor(Date.now() / 1000) + 3600,
        iat: Math.floor(Date.now() / 1000),
        jti: 'token123',
        scope: 'credential:read',
      });

      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
      });
      tokenManager = new TokenManager(client);

      vi.spyOn(client, 'post').mockResolvedValue({
        valid: true,
        claims: {
          jti: 'token123',
          sub: 'tenant1:user1',
          exp: Math.floor(Date.now() / 1000) + 3600,
          iat: Math.floor(Date.now() / 1000),
          scope: 'credential:read',
          tenant_id: 'tenant1',
        },
      });

      const isValid = await tokenManager.verify();
      expect(isValid).toBe(true);
      expect(client.post).toHaveBeenCalledWith(
        '/tokens/verify',
        { token },
        { skipRetry: true }
      );
    });

    it('应该在 Token 无效时返回 false', async () => {
      const token = createMockToken({
        sub: 'tenant1:user1',
        exp: Math.floor(Date.now() / 1000) + 3600,
        iat: Math.floor(Date.now() / 1000),
        jti: 'token123',
        scope: 'credential:read',
      });

      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
      });
      tokenManager = new TokenManager(client);

      vi.spyOn(client, 'post').mockResolvedValue({
        valid: false,
        error: 'Token has been revoked',
      });

      const isValid = await tokenManager.verify();
      expect(isValid).toBe(false);
    });

    it('应该在无 Token 时返回 false', async () => {
      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
      });
      tokenManager = new TokenManager(client);

      const isValid = await tokenManager.verify();
      expect(isValid).toBe(false);
    });

    it('应该在认证错误时返回 false', async () => {
      const token = createMockToken({
        sub: 'tenant1:user1',
        exp: Math.floor(Date.now() / 1000) + 3600,
        iat: Math.floor(Date.now() / 1000),
        jti: 'token123',
        scope: 'credential:read',
      });

      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
      });
      tokenManager = new TokenManager(client);

      vi.spyOn(client, 'post').mockRejectedValue(
        new CredBridgeError(CredBridgeErrorCode.InvalidToken, 'Invalid token', 401)
      );

      const isValid = await tokenManager.verify();
      expect(isValid).toBe(false);
    });
  });

  describe('Token 撤销', () => {
    it('应该成功撤销 Token', async () => {
      const token = createMockToken({
        sub: 'tenant1:user1',
        exp: Math.floor(Date.now() / 1000) + 3600,
        iat: Math.floor(Date.now() / 1000),
        jti: 'token123',
        scope: 'credential:read',
      });

      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
      });
      tokenManager = new TokenManager(client);

      vi.spyOn(client, 'post').mockResolvedValue({
        revoked: true,
      });

      const result = await tokenManager.revoke();
      expect(result).toBe(true);
      expect(client.post).toHaveBeenCalledWith('/tokens/token123/revoke', {}, undefined);
    });

    it('应该在无 Token 时抛出错误', async () => {
      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
      });
      tokenManager = new TokenManager(client);

      await expect(tokenManager.revoke()).rejects.toThrow(CredBridgeError);
      await expect(tokenManager.revoke()).rejects.toMatchObject({
        code: CredBridgeErrorCode.InvalidToken,
      });
    });
  });

  describe('后端对齐方法', () => {
    beforeEach(() => {
      const token = createMockToken({
        sub: 'tenant1:user1',
        exp: Math.floor(Date.now() / 1000) + 3600,
        iat: Math.floor(Date.now() / 1000),
        jti: 'token123',
        scope: 'credential:read',
      });

      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
      });
      tokenManager = new TokenManager(client);
    });

    it('应该创建新的平台 Token', async () => {
      vi.spyOn(client, 'post').mockResolvedValue({
        access_token: 'new-access-token',
        token_id: 'new-token-id',
        token_type: 'Bearer',
        expires_in: 3600,
        scope: 'credential:read',
        issued_at: 1710000000,
        expires_at: 1710003600,
      });

      const result = await tokenManager.create({
        scopes: ['credential:read'],
        expiresIn: 3600,
      });

      expect(result.accessToken).toBe('new-access-token');
      expect(result.tokenId).toBe('new-token-id');
      expect(client.post).toHaveBeenCalledWith(
        '/tokens',
        {
          scopes: ['credential:read'],
          expires_in: 3600,
        },
        undefined
      );
    });

    it('应该获取 Token 统计', async () => {
      vi.spyOn(client, 'get').mockResolvedValue({
        active_tokens: 7,
      });

      const result = await tokenManager.stats();

      expect(result.activeTokens).toBe(7);
      expect(client.get).toHaveBeenCalledWith('/tokens/stats', undefined);
    });
  });

  describe('获取 ID 和元数据', () => {
    const token = createMockToken({
      sub: 'tenant1:user1',
      exp: 1704067200,
      iat: 1704063600,
      jti: 'token123',
      scope: 'credential:read',
    });

    beforeEach(() => {
      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
      });
      tokenManager = new TokenManager(client);
    });

    it('应该返回租户 ID', () => {
      expect(tokenManager.getTenantId()).toBe('tenant1');
    });

    it('应该返回用户 ID', () => {
      expect(tokenManager.getUserId()).toBe('user1');
    });

    it('应该返回 Token ID', () => {
      expect(tokenManager.getTokenId()).toBe('token123');
    });

    it('应该返回颁发时间', () => {
      expect(tokenManager.getIssuedAt()).toBe(1704063600);
    });

    it('应该返回过期时间', () => {
      expect(tokenManager.getExpiresAt()).toBe(1704067200);
    });

    it('应该在无 Token 时返回 undefined', () => {
      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
      });
      tokenManager = new TokenManager(client);

      expect(tokenManager.getTenantId()).toBeUndefined();
      expect(tokenManager.getUserId()).toBeUndefined();
      expect(tokenManager.getTokenId()).toBeUndefined();
    });
  });

  describe('设置新 Token', () => {
    it('应该能够设置新 Token', () => {
      const oldToken = createMockToken({
        sub: 'tenant1:user1',
        exp: Math.floor(Date.now() / 1000) + 3600,
        iat: Math.floor(Date.now() / 1000),
        jti: 'token123',
        scope: 'credential:read',
      });

      client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: oldToken,
      });
      tokenManager = new TokenManager(client);

      const newToken = createMockToken({
        sub: 'tenant2:user2',
        exp: Math.floor(Date.now() / 1000) + 7200,
        iat: Math.floor(Date.now() / 1000),
        jti: 'token456',
        scope: 'credential:write',
      });

      tokenManager.setToken(newToken);

      expect(tokenManager.getTokenId()).toBe('token456');
      expect(tokenManager.getTenantId()).toBe('tenant2');
      expect(tokenManager.getUserId()).toBe('user2');
    });
  });
});
