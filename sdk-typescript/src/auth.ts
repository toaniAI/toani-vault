/**
 * CredBridge SDK - Auth 服务
 */

import type { CredBridgeClient } from "./client.js";
import {
  type AuthCreateAccessTokenRequest,
  type AuthCreateAccessTokenResponse,
  type AuthIdentityInfo,
  type AuthLogoutResponse,
  type AuthMeResponse,
  type AuthMembershipInfo,
  type AuthMembershipsResponse,
  type AuthTenantInfo,
  type AuthUserProfile,
  type RequestOptions,
} from "./types.js";

interface AuthIdentityInfoApi {
  provider: string;
  subject: string;
  wallet_address?: string;
  email?: string;
  is_verified: boolean;
  is_primary: boolean;
}

interface AuthUserProfileApi {
  id: string;
  display_name?: string;
  status: string;
  onboarding_completed: boolean;
  default_tenant_id?: string;
  identities: AuthIdentityInfoApi[];
}

interface AuthMembershipInfoApi {
  id: string;
  tenant_id: string;
  role: string;
  status: string;
  scopes: string[];
  joined_at?: string;
}

interface AuthTenantInfoApi {
  id: string;
  name?: string;
}

interface AuthMeResponseApi {
  user: AuthUserProfileApi;
  current_tenant?: AuthTenantInfoApi;
  current_membership?: AuthMembershipInfoApi;
  memberships: AuthMembershipInfoApi[];
  mfa_status: string;
}

interface AuthMembershipsResponseApi {
  memberships: AuthMembershipInfoApi[];
}

interface AuthCreateAccessTokenResponseApi {
  access_token: string;
  token_id: string;
  token_type: string;
  subject_type?: string;
  issued_from?: string;
  display_name?: string;
  expires_at: number;
  expires_in: number;
  granted_scopes: string[];
  revoked_at?: string | null;
}

function mapIdentity(identity: AuthIdentityInfoApi): AuthIdentityInfo {
  return {
    provider: identity.provider,
    subject: identity.subject,
    walletAddress: identity.wallet_address,
    email: identity.email,
    isVerified: identity.is_verified,
    isPrimary: identity.is_primary,
  };
}

function mapUser(user: AuthUserProfileApi): AuthUserProfile {
  return {
    id: user.id,
    displayName: user.display_name,
    status: user.status,
    onboardingCompleted: user.onboarding_completed,
    defaultTenantId: user.default_tenant_id,
    identities: user.identities.map(mapIdentity),
  };
}

function mapMembership(membership: AuthMembershipInfoApi): AuthMembershipInfo {
  return {
    id: membership.id,
    tenantId: membership.tenant_id,
    role: membership.role,
    status: membership.status,
    scopes: membership.scopes,
    joinedAt: membership.joined_at,
  };
}

function mapTenant(tenant?: AuthTenantInfoApi): AuthTenantInfo | undefined {
  if (!tenant) {
    return undefined;
  }

  return {
    id: tenant.id,
    name: tenant.name,
  };
}

function mapMeResponse(response: AuthMeResponseApi): AuthMeResponse {
  return {
    user: mapUser(response.user),
    currentTenant: mapTenant(response.current_tenant),
    currentMembership: response.current_membership
      ? mapMembership(response.current_membership)
      : undefined,
    memberships: response.memberships.map(mapMembership),
    mfaStatus: response.mfa_status,
  };
}

function mapCreateAccessTokenResponse(
  response: AuthCreateAccessTokenResponseApi,
): AuthCreateAccessTokenResponse {
  return {
    accessToken: response.access_token,
    tokenId: response.token_id,
    tokenType: response.token_type,
    subjectType: response.subject_type,
    issuedFrom: response.issued_from,
    displayName: response.display_name,
    expiresAt: response.expires_at,
    expiresIn: response.expires_in,
    grantedScopes: response.granted_scopes,
    revokedAt: response.revoked_at ?? undefined,
  };
}

/**
 * Auth 服务
 */
export class AuthService {
  private client: CredBridgeClient;

  constructor(client: CredBridgeClient) {
    this.client = client;
  }

  public async createAccessToken(
    request: AuthCreateAccessTokenRequest,
    options?: RequestOptions,
  ): Promise<AuthCreateAccessTokenResponse> {
    const response = await this.client.post<AuthCreateAccessTokenResponseApi>(
      "/auth/access-token",
      {
        scopes: request.scopes,
        ttl_seconds: request.ttlSeconds,
        credential_ids: request.credentialIds,
      },
      options,
    );

    return mapCreateAccessTokenResponse(response);
  }

  public async revokeAccessToken(
    tokenId: string,
    options?: RequestOptions,
  ): Promise<boolean> {
    const response = await this.client.post<{ revoked: boolean }>(
      `/tokens/${tokenId}/revoke`,
      {},
      options,
    );

    return response.revoked;
  }

  public async me(options?: RequestOptions): Promise<AuthMeResponse> {
    const response = await this.client.get<AuthMeResponseApi>(
      "/auth/me",
      options,
    );
    return mapMeResponse(response);
  }

  public async logout(options?: RequestOptions): Promise<AuthLogoutResponse> {
    return this.client.post<AuthLogoutResponse>(
      "/auth/logout",
      undefined,
      options,
    );
  }

  public async memberships(
    options?: RequestOptions,
  ): Promise<AuthMembershipsResponse> {
    const response = await this.client.get<AuthMembershipsResponseApi>(
      "/auth/memberships",
      options,
    );

    return {
      memberships: response.memberships.map(mapMembership),
    };
  }
}
