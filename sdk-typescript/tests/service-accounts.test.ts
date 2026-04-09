import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ServiceAccountsService } from '../src/service-accounts.js';
import { CredBridgeClient } from '../src/client.js';

describe('ServiceAccountsService', () => {
  const mockBaseUrl = 'https://vault.credbridge.io';
  const mockToken =
    'v4.local.eyJzdWIiOiJ0ZW5hbnQxOnVzZXIxIiwiZXhwIjoxNzA0MDY3MjAwLCJpYXQiOjE3MDQwNjM2MDAsImp0aSI6InRva2VuMTIzIiwic2NvcGUiOiJhZG1pbiJ9.signature';

  let client: CredBridgeClient;
  let service: ServiceAccountsService;

  beforeEach(() => {
    client = new CredBridgeClient({
      baseUrl: mockBaseUrl,
      token: mockToken,
    });
    service = new ServiceAccountsService(client);
    vi.resetAllMocks();
  });

  it('creates service account', async () => {
    vi.spyOn(client, 'post').mockResolvedValue({
      id: 'sa-1',
      tenant_id: 'tenant-1',
      name: 'ci-bot',
      role: 'service_account',
      scope_ceiling: ['tokens:read'],
      status: 'active',
      created_by: 'user-1',
      created_at: '2026-04-09T10:00:00Z',
      updated_at: '2026-04-09T10:00:00Z',
      deleted_at: null,
    });

    const response = await service.create({
      name: 'ci-bot',
      scopeCeiling: ['tokens:read'],
    });

    expect(response.id).toBe('sa-1');
    expect(response.scopeCeiling).toEqual(['tokens:read']);
  });

  it('lists service account token metadata', async () => {
    vi.spyOn(client, 'get').mockResolvedValue([
      {
        token_id: 'token-1',
        token_type: 'service_account_token',
        subject_type: 'service_account',
        subject_id: 'sa-1',
        tenant_id: 'tenant-1',
        issued_from: 'service_account',
        session_id: null,
        membership_id: null,
        display_name: 'ci-token',
        granted_scopes: ['credential:read'],
        expires_at: '2026-04-09T11:00:00Z',
        revoked_at: null,
        created_at: '2026-04-09T10:00:00Z',
        last_used_at: null,
      },
    ]);

    const tokens = await service.listTokens('sa-1');
    expect(tokens).toHaveLength(1);
    expect(tokens[0]?.subjectType).toBe('service_account');
    expect(tokens[0]?.displayName).toBe('ci-token');
  });
});
