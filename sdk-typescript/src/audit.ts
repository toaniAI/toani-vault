/**
 * CredBridge SDK - Audit 服务
 */

import type { CredBridgeClient } from './client.js';
import {
  type AuditExportResult,
  type AuditLogItem,
  type AuditLogsListResponse,
  type AuditVerificationDetail,
  type ExportAuditLogsRequest,
  type ListAuditLogsRequest,
  type RequestOptions,
  type VerifyAuditLogRequest,
  type VerifyAuditLogResult,
} from './types.js';

interface AuditLogItemApi {
  id: string;
  timestamp: number;
  user_id_hash: string;
  session_id: string;
  service: string;
  action: string;
  risk_tier: string;
  outcome: string;
  log_index: number;
}

interface AuditLogsListResponseApi {
  items: AuditLogItemApi[];
  total: number;
  page: number;
  page_size: number;
  total_pages: number;
}

interface AuditExportResultApi {
  export_id: string;
  format: 'json' | 'csv';
  content: string;
  integrity_hash: string;
  count: number;
  generated_at: number;
}

interface AuditVerificationDetailApi {
  step: string;
  passed: boolean;
  message?: string;
}

interface VerifyAuditLogResultApi {
  id: string;
  log_index: number;
  verified: boolean;
  content_hash_match: boolean;
  signature_valid: boolean;
  merkle_proof_valid: boolean;
  details: AuditVerificationDetailApi[];
  verified_at: number;
}

function mapLogItem(item: AuditLogItemApi): AuditLogItem {
  return {
    id: item.id,
    timestamp: item.timestamp,
    userIdHash: item.user_id_hash,
    sessionId: item.session_id,
    service: item.service,
    action: item.action,
    riskTier: item.risk_tier,
    outcome: item.outcome,
    logIndex: item.log_index,
  };
}

function mapVerificationDetail(detail: AuditVerificationDetailApi): AuditVerificationDetail {
  return {
    step: detail.step,
    passed: detail.passed,
    message: detail.message,
  };
}

/**
 * Audit 服务
 */
export class AuditService {
  private client: CredBridgeClient;

  constructor(client: CredBridgeClient) {
    this.client = client;
  }

  public async listLogs(
    request?: ListAuditLogsRequest,
    options?: RequestOptions
  ): Promise<AuditLogsListResponse> {
    const query = new URLSearchParams();
    if (request?.startTime !== undefined) query.set('start_time', String(request.startTime));
    if (request?.endTime !== undefined) query.set('end_time', String(request.endTime));
    if (request?.userIdHash) query.set('user_id_hash', request.userIdHash);
    if (request?.action) query.set('action', request.action);
    if (request?.riskTier) query.set('risk_tier', request.riskTier);
    if (request?.outcome) query.set('outcome', request.outcome);
    if (request?.service) query.set('service', request.service);
    if (request?.page !== undefined) query.set('page', String(request.page));
    if (request?.pageSize !== undefined) query.set('page_size', String(request.pageSize));

    const path = query.toString() ? `/audit/logs?${query.toString()}` : '/audit/logs';
    const response = await this.client.get<AuditLogsListResponseApi>(path, options);

    return {
      items: response.items.map(mapLogItem),
      total: response.total,
      page: response.page,
      pageSize: response.page_size,
      totalPages: response.total_pages,
    };
  }

  public async exportLogs(
    request: ExportAuditLogsRequest,
    options?: RequestOptions
  ): Promise<AuditExportResult> {
    const response = await this.client.post<AuditExportResultApi>(
      '/audit/export',
      {
        start_time: request.startTime,
        end_time: request.endTime,
        format: request.format,
        user_id_hash: request.userIdHash,
        action: request.action,
      },
      options
    );

    return {
      exportId: response.export_id,
      format: response.format,
      content: response.content,
      integrityHash: response.integrity_hash,
      count: response.count,
      generatedAt: response.generated_at,
    };
  }

  public async verifyLog(
    request: VerifyAuditLogRequest,
    options?: RequestOptions
  ): Promise<VerifyAuditLogResult> {
    const response = await this.client.post<VerifyAuditLogResultApi>(
      '/audit/verify',
      {
        id: request.id,
        log_index: request.logIndex,
      },
      options
    );

    return {
      id: response.id,
      logIndex: response.log_index,
      verified: response.verified,
      contentHashMatch: response.content_hash_match,
      signatureValid: response.signature_valid,
      merkleProofValid: response.merkle_proof_valid,
      details: response.details.map(mapVerificationDetail),
      verifiedAt: response.verified_at,
    };
  }
}
