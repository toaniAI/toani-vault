/**
 * CredBridge SDK - 审批服务
 *
 * 提供审批异步发起和状态查询功能。
 */

import type { CredBridgeClient } from "./client.js";
import type {
  ApprovalDetailResponse,
  ApprovalInitiationResponse,
  CreateApprovalRequest,
  RequestOptions,
} from "./types.js";

interface CreateApprovalResponseApi {
  approval_id: string;
  status: string;
}

/**
 * 审批服务
 */
export class ApprovalsService {
  private client: CredBridgeClient;

  constructor(client: CredBridgeClient) {
    this.client = client;
  }

  /**
   * 异步发起审批请求。
   */
  public async create(
    request: CreateApprovalRequest,
    options?: RequestOptions,
  ): Promise<ApprovalInitiationResponse> {
    const response = await this.client.post<CreateApprovalResponseApi>(
      "/approvals",
      {
        business_type: request.businessType,
        business_id: request.businessId,
      },
      options,
    );

    return {
      approval_id: response.approval_id,
      status: response.status,
      business_type: request.businessType,
      business_id: request.businessId,
    };
  }

  /**
   * 查询审批当前状态。
   */
  public async get(
    approvalId: string,
    options?: RequestOptions,
  ): Promise<ApprovalDetailResponse> {
    return this.client.get<ApprovalDetailResponse>(
      `/approvals/${approvalId}`,
      options,
    );
  }
}
