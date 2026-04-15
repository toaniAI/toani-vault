/**
 * CredBridge SDK WebSocket 客户端
 *
 * 提供安全的 WebSocket 连接，用于实时控制和监控 TEE 沙箱会话
 *
 * @example
 * ```typescript
 * const client = new SandboxWebSocketClient({
 *   baseUrl: 'wss://api.credbridge.example.com',
 *   token: 'your-paseto-token',
 *   sessionId: 'session-uuid',
 *   credentialId: 'credential-uuid',
 * });
 *
 * client.onConnected = (data) => {
 *   console.log('Connected:', data);
 * };
 *
 * client.onOperationProgress = (data) => {
 *   console.log('Progress:', data.progress);
 * };
 *
 * await client.connect();
 *
 * const result = await client.executeOperation({
 *   operationType: 'navigate',
 *   description: 'Navigate to example.com',
 *   parameters: { url: 'https://example.com' },
 * });
 * ```
 */

import { CredBridgeError, CredBridgeErrorCode } from "./types.js";

/** WebSocket 配置选项 */
export interface WebSocketConfig {
  /** API 基础 URL */
  baseUrl: string;
  /** API Token (PASETO v4.local) */
  token: string;
  /** 会话 ID */
  sessionId: string;
  /** 凭证 ID */
  credentialId: string;
  /** 心跳间隔（毫秒，默认 30000） */
  heartbeatInterval?: number;
  /** 操作超时（毫秒，默认 60000） */
  operationTimeout?: number;
  /** 自动重连（默认 true） */
  autoReconnect?: boolean;
  /** 最大重连次数（默认 3） */
  maxReconnectAttempts?: number;
  /** 重连延迟（毫秒，默认 5000） */
  reconnectDelay?: number;
}

/** 客户端消息类型 */
export interface ClientMessage {
  type: "execute" | "heartbeat" | "close";
}

/** 执行操作消息 */
export interface ExecuteMessage extends ClientMessage {
  type: "execute";
  operation_id?: string;
  operation_type: string;
  description: string;
  parameters?: Record<string, unknown>;
}

/** 心跳消息 */
export interface HeartbeatMessage extends ClientMessage {
  type: "heartbeat";
  timestamp: number;
}

/** 关闭消息 */
export interface CloseMessage extends ClientMessage {
  type: "close";
  reason?: string;
}

/** 服务端消息类型 */
export type ServerMessage =
  | ConnectedMessage
  | OperationProgressMessage
  | OperationCompletedMessage
  | HeartbeatAckMessage
  | SessionStatusUpdateMessage
  | ErrorMessage;

/** 连接成功消息 */
export interface ConnectedMessage {
  type: "connected";
  session_id: string;
  connected_at: string;
  heartbeat_interval: number;
}

/** 操作进度消息 */
export interface OperationProgressMessage {
  type: "operation_progress";
  operation_id: string;
  operation_type: string;
  status: string;
  progress: number;
  message?: string;
  timestamp: string;
}

/** 操作完成消息 */
export interface OperationCompletedMessage {
  type: "operation_completed";
  operation_id: string;
  operation_type: string;
  success: boolean;
  data?: Record<string, unknown>;
  error?: string;
  execution_time_ms: number;
  timestamp: string;
}

/** 心跳确认消息 */
export interface HeartbeatAckMessage {
  type: "heartbeat_ack";
  client_timestamp: number;
  server_timestamp: number;
}

/** 会话状态更新消息 */
export interface SessionStatusUpdateMessage {
  type: "session_status_update";
  session_id: string;
  status: string;
  message?: string;
  timestamp: string;
}

/** 错误消息 */
export interface ErrorMessage {
  type: "error";
  code: string;
  message: string;
  operation_id?: string;
  timestamp: string;
}

/** 执行操作选项 */
export interface ExecuteOperationOptions {
  /** 操作类型 */
  operationType: string;
  /** 操作描述 */
  description: string;
  /** 操作参数 */
  parameters?: Record<string, unknown>;
  /** 自定义操作 ID */
  operationId?: string;
  /** 超时（毫秒） */
  timeout?: number;
}

/** 执行操作结果 */
export interface ExecuteOperationResult {
  operationId: string;
  success: boolean;
  data?: Record<string, unknown>;
  error?: string;
  executionTimeMs: number;
}

/** WebSocket 连接状态 */
export enum WebSocketState {
  /** 未连接 */
  Disconnected = "disconnected",
  /** 正在连接 */
  Connecting = "connecting",
  /** 已连接 */
  Connected = "connected",
  /** 正在重连 */
  Reconnecting = "reconnecting",
  /** 已关闭 */
  Closed = "closed",
}

/**
 * 沙箱 WebSocket 客户端
 */
export class SandboxWebSocketClient {
  private config: Required<WebSocketConfig>;
  private ws: WebSocket | null = null;
  private state: WebSocketState = WebSocketState.Disconnected;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private reconnectAttempts = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private pendingOperations = new Map<
    string,
    {
      resolve: (value: ExecuteOperationResult) => void;
      reject: (reason: CredBridgeError) => void;
      timeout: ReturnType<typeof setTimeout>;
    }
  >();
  // 事件回调
  /** 连接成功回调 */
  onConnected?: (data: ConnectedMessage) => void;
  /** 连接关闭回调 */
  onDisconnected?: (code: number, reason: string) => void;
  /** 操作进度回调 */
  onOperationProgress?: (data: OperationProgressMessage) => void;
  /** 操作完成回调 */
  onOperationCompleted?: (data: OperationCompletedMessage) => void;
  /** 会话状态更新回调 */
  onSessionStatusUpdate?: (data: SessionStatusUpdateMessage) => void;
  /** 错误回调 */
  onError?: (error: ErrorMessage) => void;
  /** 连接错误回调 */
  onConnectionError?: (error: Event) => void;
  /** 重连回调 */
  onReconnecting?: (attempt: number, maxAttempts: number) => void;

  constructor(config: WebSocketConfig) {
    this.config = {
      heartbeatInterval: 30000,
      operationTimeout: 60000,
      autoReconnect: true,
      maxReconnectAttempts: 3,
      reconnectDelay: 5000,
      ...config,
    };
  }

  /**
   * 获取当前连接状态
   */
  public getState(): WebSocketState {
    return this.state;
  }

  /**
   * 检查是否已连接
   */
  public isConnected(): boolean {
    return (
      this.state === WebSocketState.Connected &&
      this.ws?.readyState === WebSocket.OPEN
    );
  }

  /**
   * 连接到 WebSocket
   */
  public async connect(): Promise<void> {
    if (
      this.state === WebSocketState.Connected ||
      this.state === WebSocketState.Connecting
    ) {
      return;
    }

    this.state = WebSocketState.Connecting;

    return new Promise((resolve, reject) => {
      try {
        // 构建 WebSocket URL
        const wsUrl = this.buildWebSocketUrl();

        this.ws = new WebSocket(wsUrl);

        this.ws.onopen = () => {
          this.state = WebSocketState.Connected;
          this.reconnectAttempts = 0;
          this.startHeartbeat();
          resolve();
        };

        this.ws.onmessage = (event) => {
          this.handleMessage(event.data);
        };

        this.ws.onclose = (event) => {
          this.handleClose(event.code, event.reason);
        };

        this.ws.onerror = (error) => {
          this.handleError(error);
          if (this.state === WebSocketState.Connecting) {
            reject(
              new CredBridgeError(
                CredBridgeErrorCode.NetworkError,
                "WebSocket connection failed",
              ),
            );
          }
        };
      } catch (error) {
        this.state = WebSocketState.Disconnected;
        reject(
          new CredBridgeError(
            CredBridgeErrorCode.NetworkError,
            `Failed to create WebSocket: ${error instanceof Error ? error.message : "Unknown error"}`,
          ),
        );
      }
    });
  }

  /**
   * 断开连接
   */
  public disconnect(reason?: string): void {
    this.stopHeartbeat();
    this.clearReconnectTimer();

    if (this.ws) {
      // 发送关闭消息
      if (this.ws.readyState === WebSocket.OPEN) {
        const message: CloseMessage = {
          type: "close",
          reason,
        };
        this.sendMessage(message);
      }

      this.ws.close(1000, reason || "Client disconnect");
      this.ws = null;
    }

    this.state = WebSocketState.Closed;
    this.rejectAllPending("Client disconnected");
  }

  /**
   * 执行操作
   */
  public async executeOperation(
    options: ExecuteOperationOptions,
  ): Promise<ExecuteOperationResult> {
    if (!this.isConnected()) {
      throw new CredBridgeError(
        CredBridgeErrorCode.NetworkError,
        "WebSocket is not connected",
      );
    }

    const operationId = options.operationId || this.generateId();
    const timeout = options.timeout || this.config.operationTimeout;

    return new Promise((resolve, reject) => {
      // 设置超时
      const timeoutId = setTimeout(() => {
        this.pendingOperations.delete(operationId);
        reject(
          new CredBridgeError(
            CredBridgeErrorCode.Timeout,
            `Operation ${operationId} timed out after ${timeout}ms`,
          ),
        );
      }, timeout);

      // 保存 pending 操作
      this.pendingOperations.set(operationId, {
        resolve,
        reject,
        timeout: timeoutId,
      });

      // 发送执行消息
      const message: ExecuteMessage = {
        type: "execute",
        operation_id: operationId,
        operation_type: options.operationType,
        description: options.description,
        parameters: options.parameters || {},
      };

      this.sendMessage(message);
    });
  }

  /**
   * 发送心跳
   */
  private sendHeartbeat(): void {
    if (!this.isConnected()) return;

    const message: HeartbeatMessage = {
      type: "heartbeat",
      timestamp: Date.now(),
    };

    this.sendMessage(message);
  }

  /**
   * 发送消息
   */
  private sendMessage(message: ClientMessage): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new CredBridgeError(
        CredBridgeErrorCode.NetworkError,
        "WebSocket is not open",
      );
    }

    this.ws.send(JSON.stringify(message));
  }

  /**
   * 处理收到的消息
   */
  private handleMessage(data: string): void {
    try {
      const message = JSON.parse(data) as ServerMessage;

      switch (message.type) {
        case "connected":
          this.onConnected?.(message);
          break;

        case "operation_progress":
          this.onOperationProgress?.(message);
          break;

        case "operation_completed":
          this.handleOperationCompleted(message);
          break;

        case "heartbeat_ack":
          // 心跳确认，无需处理
          break;

        case "session_status_update":
          this.onSessionStatusUpdate?.(message);
          break;

        case "error":
          this.handleErrorMessage(message);
          break;

        default:
          console.warn(
            "Unknown message type:",
            (message as { type: string }).type,
          );
      }
    } catch (error) {
      console.error("Failed to parse message:", error);
    }
  }

  /**
   * 处理操作完成消息
   */
  private handleOperationCompleted(message: OperationCompletedMessage): void {
    const pending = this.pendingOperations.get(message.operation_id);
    if (pending) {
      clearTimeout(pending.timeout);
      this.pendingOperations.delete(message.operation_id);

      pending.resolve({
        operationId: message.operation_id,
        success: message.success,
        data: message.data,
        error: message.error,
        executionTimeMs: message.execution_time_ms,
      });
    }

    this.onOperationCompleted?.(message);
  }

  /**
   * 处理错误消息
   */
  private handleErrorMessage(message: ErrorMessage): void {
    // 如果有相关的 pending 操作，reject 它
    if (message.operation_id) {
      const pendingOp = this.pendingOperations.get(message.operation_id);
      if (pendingOp) {
        clearTimeout(pendingOp.timeout);
        this.pendingOperations.delete(message.operation_id);
        pendingOp.reject(
          new CredBridgeError(
            CredBridgeErrorCode.InternalError,
            message.message,
          ),
        );
        return;
      }
    }

    this.onError?.(message);
  }

  /**
   * 处理连接关闭
   */
  private handleClose(code: number, reason: string): void {
    this.stopHeartbeat();
    this.ws = null;

    if (this.state !== WebSocketState.Closed) {
      this.state = WebSocketState.Disconnected;
      this.onDisconnected?.(code, reason);

      // 尝试重连
      if (
        this.config.autoReconnect &&
        this.reconnectAttempts < this.config.maxReconnectAttempts
      ) {
        this.scheduleReconnect();
      } else {
        this.rejectAllPending("Connection closed");
      }
    }
  }

  /**
   * 处理连接错误
   */
  private handleError(error: Event): void {
    this.onConnectionError?.(error);
  }

  /**
   * 启动心跳
   */
  private startHeartbeat(): void {
    this.stopHeartbeat();
    this.heartbeatTimer = setInterval(() => {
      this.sendHeartbeat();
    }, this.config.heartbeatInterval);
  }

  /**
   * 停止心跳
   */
  private stopHeartbeat(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  /**
   * 安排重连
   */
  private scheduleReconnect(): void {
    this.reconnectAttempts++;
    this.state = WebSocketState.Reconnecting;

    this.onReconnecting?.(
      this.reconnectAttempts,
      this.config.maxReconnectAttempts,
    );

    this.clearReconnectTimer();
    this.reconnectTimer = setTimeout(() => {
      this.connect().catch(() => {
        // 重连失败，继续尝试
        if (this.reconnectAttempts < this.config.maxReconnectAttempts) {
          this.scheduleReconnect();
        }
      });
    }, this.config.reconnectDelay);
  }

  /**
   * 清除重连定时器
   */
  private clearReconnectTimer(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  /**
   * 拒绝所有 pending 的操作
   */
  private rejectAllPending(reason: string): void {
    const error = new CredBridgeError(CredBridgeErrorCode.NetworkError, reason);

    for (const [, pending] of this.pendingOperations) {
      clearTimeout(pending.timeout);
      pending.reject(error);
    }
    this.pendingOperations.clear();

  }

  /**
   * 构建 WebSocket URL
   */
  private buildWebSocketUrl(): string {
    // 将 http/https 转换为 ws/wss
    let baseUrl = this.config.baseUrl.replace(/^http/, "ws");
    baseUrl = baseUrl.replace(/\/$/, "");

    return `${baseUrl}/api/v1/sandbox/sessions/${this.config.sessionId}/ws/${this.config.credentialId}`;
  }

  /**
   * 生成唯一 ID
   */
  private generateId(): string {
    return `op_${Date.now()}_${Math.random().toString(36).substring(2, 11)}`;
  }
}

export default SandboxWebSocketClient;
