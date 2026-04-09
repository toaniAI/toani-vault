import type { CredBridgeClient } from './client.js';
import type {
  RequestOptions,
  ServiceAccountInfo,
  ServiceAccountTokenCreateRequest,
  ServiceAccountTokenCreateResponse,
  ServiceAccountTokenMetadata,
  CreateServiceAccountRequest,
  UpdateServiceAccountRequest,
} from './types.js';

interface ServiceAccountInfoApi {
  id: string;
  tenant_id: string;
  name: string;
  description?: string;
  role: string;
  scope_ceiling: string[];
  status: string;
  created_by: string;
  created_at: string;
  updated_at: string;
  deleted_at?: string | null;
}

interface ServiceAccountTokenCreateResponseApi {
  access_token: string;
  token_id: string;
  token_type: string;
  subject_type: string;
  issued_from: string;
  display_name?: string;
  expires_in: number;
  scope: string;
  granted_scopes: string[];
  issued_at: number;
  expires_at: number;
  revoked_at?: string | null;
}

interface ServiceAccountTokenMetadataApi {
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

function mapServiceAccount(item: ServiceAccountInfoApi): ServiceAccountInfo {
  return {
    id: item.id,
    tenantId: item.tenant_id,
    name: item.name,
    description: item.description,
    role: item.role,
    scopeCeiling: item.scope_ceiling,
    status: item.status,
    createdBy: item.created_by,
    createdAt: item.created_at,
    updatedAt: item.updated_at,
    deletedAt: item.deleted_at ?? undefined,
  };
}

function mapTokenCreateResponse(
  item: ServiceAccountTokenCreateResponseApi
): ServiceAccountTokenCreateResponse {
  return {
    accessToken: item.access_token,
    tokenId: item.token_id,
    tokenType: item.token_type,
    subjectType: item.subject_type,
    issuedFrom: item.issued_from,
    displayName: item.display_name,
    expiresIn: item.expires_in,
    scope: item.scope,
    grantedScopes: item.granted_scopes,
    issuedAt: item.issued_at,
    expiresAt: item.expires_at,
    revokedAt: item.revoked_at ?? undefined,
  };
}

function mapTokenMetadata(item: ServiceAccountTokenMetadataApi): ServiceAccountTokenMetadata {
  return {
    tokenId: item.token_id,
    tokenType: item.token_type,
    subjectType: item.subject_type,
    subjectId: item.subject_id,
    tenantId: item.tenant_id,
    issuedFrom: item.issued_from,
    sessionId: item.session_id ?? undefined,
    membershipId: item.membership_id ?? undefined,
    displayName: item.display_name ?? undefined,
    grantedScopes: item.granted_scopes,
    expiresAt: item.expires_at,
    revokedAt: item.revoked_at ?? undefined,
    createdAt: item.created_at,
    lastUsedAt: item.last_used_at ?? undefined,
  };
}

export class ServiceAccountsService {
  private client: CredBridgeClient;

  constructor(client: CredBridgeClient) {
    this.client = client;
  }

  public async create(
    request: CreateServiceAccountRequest,
    options?: RequestOptions
  ): Promise<ServiceAccountInfo> {
    const response = await this.client.post<ServiceAccountInfoApi>(
      '/service-accounts',
      {
        name: request.name,
        description: request.description,
        scope_ceiling: request.scopeCeiling,
      },
      options
    );
    return mapServiceAccount(response);
  }

  public async list(options?: RequestOptions): Promise<ServiceAccountInfo[]> {
    const response = await this.client.get<ServiceAccountInfoApi[]>(
      '/service-accounts',
      options
    );
    return response.map(mapServiceAccount);
  }

  public async get(id: string, options?: RequestOptions): Promise<ServiceAccountInfo> {
    const response = await this.client.get<ServiceAccountInfoApi>(
      `/service-accounts/${id}`,
      options
    );
    return mapServiceAccount(response);
  }

  public async update(
    id: string,
    request: UpdateServiceAccountRequest,
    options?: RequestOptions
  ): Promise<ServiceAccountInfo> {
    const response = await this.client.patch<ServiceAccountInfoApi>(
      `/service-accounts/${id}`,
      {
        name: request.name,
        description: request.description,
        status: request.status,
        scope_ceiling: request.scopeCeiling,
      },
      options
    );
    return mapServiceAccount(response);
  }

  public async createToken(
    id: string,
    request: ServiceAccountTokenCreateRequest,
    options?: RequestOptions
  ): Promise<ServiceAccountTokenCreateResponse> {
    const response = await this.client.post<ServiceAccountTokenCreateResponseApi>(
      `/service-accounts/${id}/tokens`,
      {
        scopes: request.scopes,
        ttl_seconds: request.ttlSeconds,
        display_name: request.displayName,
      },
      options
    );
    return mapTokenCreateResponse(response);
  }

  public async listTokens(
    id: string,
    options?: RequestOptions
  ): Promise<ServiceAccountTokenMetadata[]> {
    const response = await this.client.get<ServiceAccountTokenMetadataApi[]>(
      `/service-accounts/${id}/tokens`,
      options
    );
    return response.map(mapTokenMetadata);
  }
}
