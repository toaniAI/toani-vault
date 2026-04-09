import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CredBridgeClient } from '../src/client.js';
import { TokenManager } from '../src/token.js';

describe('TokenManager metadata APIs', () => {
  const mockBaseUrl = 'https://vault.credbridge.io';
  const mockToken =
    'v4.local.eyJzdWIiOiJ0ZW5hbnQxOnVzZXIxIiwiZXhwIjoxNzA0MDY3MjAwLCJpYXQiOjE3MDQwNjM2MDAsImp0aSI6InRva2VuMTIzIiwic2NvcGUiOiJ0b2tlbnM6cmVhZCB0b2tlbnM6cmV2b2tlIn0.signature';

  let client: CredBridgeClient;
  let service: TokenManager;

  beforeEach(() => {
    client = new CredBridgeClient({ baseUrl: mockBaseUrl, token: mockToken });
    service = new TokenManager(client);
    vi.resetAllMocks();
  });

  it('lists token metadata', async () => {
    vi.spyOn(client, 'get').mockResolvedValue([
      {
        token_id: 'token-1',
        token_type: 'user_access_token',
        subject_type: 'user',
        subject_id: 'user-1',
        tenant_id: 'tenant-1',
        issued_from: 'session',
        granted_scopes: ['tokens:read'],
        expires_at: '2026-04-09T11:00:00Z',
        created_at: '2026-04-09T10:00:00Z',
      },
    ]);

    const result = await service.list();
    expect(result).toHaveLength(1);
    expect(result[0]?.tokenId).toBe('token-1');
    expect(result[0]?.subjectType).toBe('user');
  });

  it('revokes token by id', async () => {
    vi.spyOn(client, 'post').mockResolvedValue({ revoked: true });
    const result = await service.revokeById('token-1');
    expect(result).toEqual({ revoked: true, tokenId: 'token-1' });
  });
});
