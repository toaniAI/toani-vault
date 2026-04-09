/**
 * CredBridge SDK Audit 服务测试
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { AuditService } from '../src/audit.js';
import { CredBridgeClient } from '../src/client.js';

describe('AuditService', () => {
  const mockBaseUrl = 'https://vault.credbridge.io';
  const mockToken = 'v4.local.eyJzdWIiOiJ0ZW5hbnQxOnVzZXIxIiwiZXhwIjoxNzA0MDY3MjAwLCJpYXQiOjE3MDQwNjM2MDAsImp0aSI6InRva2VuMTIzIiwic2NvcGUiOiJhdWRpdDpyZWFkIn0.signature';

  let client: CredBridgeClient;
  let service: AuditService;

  beforeEach(() => {
    client = new CredBridgeClient({
      baseUrl: mockBaseUrl,
      token: mockToken,
    });
    service = new AuditService(client);
    vi.resetAllMocks();
  });

  it('应该查询审计日志并映射分页字段', async () => {
    vi.spyOn(client, 'get').mockResolvedValue({
      items: [
        {
          id: 'log-123',
          timestamp: 1710000000000,
          user_id_hash: 'hash-123',
          session_id: 'session-123',
          service: 'auth',
          action: 'token_issue',
          risk_tier: 'low',
          outcome: 'success',
          log_index: 10,
        },
      ],
      total: 1,
      page: 2,
      page_size: 5,
      total_pages: 1,
    });

    const result = await service.listLogs({
      userIdHash: 'hash-123',
      action: 'token_issue',
      page: 2,
      pageSize: 5,
    });

    expect(result.items[0]?.userIdHash).toBe('hash-123');
    expect(result.pageSize).toBe(5);
    expect(result.totalPages).toBe(1);
    expect(client.get).toHaveBeenCalledWith(
      '/audit/logs?user_id_hash=hash-123&action=token_issue&page=2&page_size=5',
      undefined
    );
  });

  it('应该导出审计日志', async () => {
    vi.spyOn(client, 'post').mockResolvedValue({
      export_id: 'export-123',
      format: 'json',
      content: 'eyJsb2dzIjpbXX0=',
      integrity_hash: 'abc123',
      count: 1,
      generated_at: 1710000000000,
    });

    const result = await service.exportLogs({
      format: 'json',
      userIdHash: 'hash-123',
    });

    expect(result.exportId).toBe('export-123');
    expect(result.integrityHash).toBe('abc123');
    expect(client.post).toHaveBeenCalledWith(
      '/audit/export',
      {
        start_time: undefined,
        end_time: undefined,
        format: 'json',
        user_id_hash: 'hash-123',
        action: undefined,
      },
      undefined
    );
  });

  it('应该验证审计日志', async () => {
    vi.spyOn(client, 'post').mockResolvedValue({
      id: 'log-123',
      log_index: 10,
      verified: true,
      content_hash_match: true,
      signature_valid: true,
      merkle_proof_valid: true,
      details: [
        {
          step: 'signature',
          passed: true,
        },
      ],
      verified_at: 1710000000000,
    });

    const result = await service.verifyLog({
      logIndex: 10,
    });

    expect(result.verified).toBe(true);
    expect(result.logIndex).toBe(10);
    expect(result.details[0]?.step).toBe('signature');
    expect(client.post).toHaveBeenCalledWith(
      '/audit/verify',
      {
        id: undefined,
        log_index: 10,
      },
      undefined
    );
  });
});
