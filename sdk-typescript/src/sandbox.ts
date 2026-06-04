/**
 * CredBridge SDK - Sandbox 服务
 *
 * 提供 TEE 安全沙箱中的受控执行功能
 */

import type { CredBridgeClient } from "./client.js";
import {
  type ExecuteOperationRequest,
  type ExecuteOperationResponse,
  type RequestOptions,
  type SandboxOperationInfo,
  OperationType,
  OperationStatus,
} from "./types.js";

interface SandboxOperationInfoApi {
  operation_id: string;
  session_id: string;
  operation_type: string;
  status: string;
  started_at: string;
  completed_at?: string;
  execution_time_ms?: number;
}

interface ExecuteOperationResponseApi {
  operation_id?: string;
  operationId?: string;
  success?: boolean;
  status?: OperationStatus;
  data?: unknown;
  result?: unknown;
  error?: string;
  execution_time_ms?: number;
  executionTimeMs?: number;
}

/**
 * Sandbox 服务
 *
 * 管理 TEE 安全沙箱中的受控操作
 */
export class SandboxService {
  private client: CredBridgeClient;

  constructor(client: CredBridgeClient) {
    this.client = client;
  }

  /**
   * 提交无会话 broker 请求
   *
   * @param request - 执行操作请求
   * @param options - 请求选项
   * @returns 操作结果
   */
  public async request(
    request: ExecuteOperationRequest,
    options?: RequestOptions,
  ): Promise<ExecuteOperationResponse> {
    const parameters: Record<string, unknown> = { ...(request.parameters ?? {}) };
    if (request.method !== undefined) parameters.method = request.method;
    if (request.headers !== undefined) parameters.headers = request.headers;
    if (request.body !== undefined) parameters.body = request.body;
    if (request.timeout !== undefined) parameters.timeout_ms = request.timeout;

    const response = await this.client.post<ExecuteOperationResponseApi>(
      "/sandbox/http-requests",
      {
        operation_type: request.operationType,
        credential_id: request.credentialId,
        service_id: request.serviceId,
        request_id: request.requestId,
        description: request.description ?? request.operationType,
        parameters,
      },
      options,
    );
    const data = response.data ?? response.result;
    const success =
      response.success ??
      (response.status !== undefined
        ? response.status === OperationStatus.Success
        : response.error === undefined);
    return {
      operationId: response.operation_id ?? response.operationId ?? "",
      success,
      status: response.status,
      data,
      result: data,
      error: response.error,
      executionTimeMs:
        response.execution_time_ms ?? response.executionTimeMs ?? 0,
    };
  }

  /**
   * 获取 broker 请求详情
   */
  public async getRequest(
    operationId: string,
    options?: RequestOptions,
  ): Promise<SandboxOperationInfo> {
    const response = await this.client.get<SandboxOperationInfoApi>(
      `/sandbox/http-requests/${operationId}`,
      options,
    );

    return {
      operationId: response.operation_id,
      sessionId: response.session_id,
      operationType: response.operation_type,
      status: response.status,
      startedAt: response.started_at,
      completedAt: response.completed_at,
      executionTimeMs: response.execution_time_ms,
    };
  }

}

export { OperationType, OperationStatus };
