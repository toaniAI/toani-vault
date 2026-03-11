/**
 * CredBridge TypeScript SDK - Express 集成示例
 *
 * 展示如何在 Express 应用中集成 CredBridge SDK
 */

import express, { Request, Response, NextFunction } from 'express';
import {
  CredBridgeClient,
  CredBridgeError,
  CredBridgeErrorCode,
  CredentialType,
} from '@credbridge/sdk';

// 扩展 Express Request 类型
declare global {
  namespace Express {
    interface Request {
      credbridge?: CredBridgeClient;
      tenantId?: string;
      userId?: string;
    }
  }
}

const app = express();
app.use(express.json());

const BASE_URL = process.env.CREDBRIDGE_BASE_URL || 'https://api.credbridge.io';

// 初始化 CredBridge 客户端中间件
function initCredBridge() {
  return (req: Request, _res: Response, next: NextFunction) => {
    const token = req.headers.authorization?.replace('Bearer ', '');

    if (!token) {
      return res.status(401).json({ error: 'Missing authorization token' });
    }

    req.credbridge = new CredBridgeClient({
      baseUrl: BASE_URL,
      token,
    });

    // 从 Token 中提取租户和用户 ID
    const tokenInfo = req.credbridge.getTokenInfo();
    if (tokenInfo) {
      req.tenantId = tokenInfo.tenantId;
      req.userId = tokenInfo.userId;
    }

    next();
  };
}

// 权限检查中间件
function requireScopes(...scopes: string[]) {
  return (req: Request, res: Response, next: NextFunction) => {
    if (!req.credbridge) {
      return res.status(500).json({ error: 'CredBridge client not initialized' });
    }

    const token = req.credbridge.token;
    const hasScopes = token.hasAllScopes(scopes as any);

    if (!hasScopes) {
      return res.status(403).json({
        error: 'Insufficient permissions',
        required: scopes,
        granted: token.getScopes(),
      });
    }

    next();
  };
}

// 应用中间件
app.use(initCredBridge());

// 健康检查
app.get('/health', (_req, res) => {
  res.json({ status: 'ok', timestamp: new Date().toISOString() });
});

// 获取当前用户信息
app.get('/api/me', (req, res) => {
  const token = req.credbridge!.token;
  res.json({
    tenantId: req.tenantId,
    userId: req.userId,
    scopes: token.getScopes(),
    expiresIn: token.getRemainingTimeFormatted(),
  });
});

// 获取凭证列表（需要 read 权限）
app.get(
  '/api/credentials',
  requireScopes('credential:read'),
  async (req, res, next) => {
    try {
      const { credentials, total } = await req.credbridge!.credentials.list();
      res.json({
        credentials,
        total,
        tenantId: req.tenantId,
      });
    } catch (error) {
      next(error);
    }
  }
);

// 获取单个凭证（需要 read 权限）
app.get(
  '/api/credentials/:id',
  requireScopes('credential:read'),
  async (req, res, next) => {
    try {
      const credential = await req.credbridge!.credentials.get(req.params.id);
      res.json(credential);
    } catch (error) {
      next(error);
    }
  }
);

// 解密凭证（需要 decrypt 权限）
app.post(
  '/api/credentials/:id/decrypt',
  requireScopes('credential:decrypt'),
  async (req, res, next) => {
    try {
      const { reason } = req.body;
      if (!reason) {
        return res.status(400).json({ error: 'Decryption reason is required' });
      }

      const decrypted = await req.credbridge!.credentials.decrypt(
        req.params.id,
        reason
      );
      res.json(decrypted);
    } catch (error) {
      next(error);
    }
  }
);

// 创建凭证（需要 write 权限）
app.post(
  '/api/credentials',
  requireScopes('credential:write'),
  async (req, res, next) => {
    try {
      const { serviceId, credentialType, plaintextData, expiresAt } = req.body;

      if (!serviceId || !credentialType || !plaintextData) {
        return res.status(400).json({
          error: 'Missing required fields: serviceId, credentialType, plaintextData',
        });
      }

      const type = credentialType as CredentialType;
      let credential;

      switch (type) {
        case CredentialType.UsernamePassword:
          credential = await req.credbridge!.credentials.createUsernamePassword(
            serviceId,
            plaintextData.username,
            plaintextData.password,
            { expiresAt }
          );
          break;
        case CredentialType.ApiKey:
          credential = await req.credbridge!.credentials.createApiKey(
            serviceId,
            plaintextData.apiKey,
            plaintextData.apiSecret,
            { expiresAt }
          );
          break;
        default:
          credential = await req.credbridge!.credentials.create({
            serviceId,
            credentialType: type,
            plaintextData,
            expiresAt,
          });
      }

      res.status(201).json(credential);
    } catch (error) {
      next(error);
    }
  }
);

// 删除凭证（需要 write 权限）
app.delete(
  '/api/credentials/:id',
  requireScopes('credential:write'),
  async (req, res, next) => {
    try {
      const result = await req.credbridge!.credentials.delete(req.params.id);
      if (result.deleted) {
        res.status(204).send();
      } else {
        res.status(404).json({ error: 'Credential not found' });
      }
    } catch (error) {
      next(error);
    }
  }
);

// 错误处理中间件
app.use((err: Error, _req: Request, res: Response, _next: NextFunction) => {
  if (err instanceof CredBridgeError) {
    const statusMap: Record<CredBridgeErrorCode, number> = {
      [CredBridgeErrorCode.Unauthorized]: 401,
      [CredBridgeErrorCode.Forbidden]: 403,
      [CredBridgeErrorCode.NotFound]: 404,
      [CredBridgeErrorCode.InvalidRequest]: 400,
      [CredBridgeErrorCode.InternalError]: 500,
      [CredBridgeErrorCode.TokenExpired]: 401,
      [CredBridgeErrorCode.InsufficientScope]: 403,
      [CredBridgeErrorCode.CredentialExpired]: 410,
      [CredBridgeErrorCode.NetworkError]: 503,
      [CredBridgeErrorCode.Timeout]: 504,
    };

    const statusCode = statusMap[err.code] || 500;

    return res.status(statusCode).json({
      error: err.message,
      code: err.code,
      requestId: err.requestId,
    });
  }

  res.status(500).json({ error: err.message });
});

// 启动服务器
const PORT = process.env.PORT || 3000;

app.listen(PORT, () => {
  console.log(`Express server running on port ${PORT}`);
  console.log(`CredBridge API: ${BASE_URL}`);
  console.log('');
  console.log('API Endpoints:');
  console.log('  GET  /health                    - 健康检查');
  console.log('  GET  /api/me                    - 获取当前用户信息');
  console.log('  GET  /api/credentials           - 获取凭证列表');
  console.log('  GET  /api/credentials/:id       - 获取凭证详情');
  console.log('  POST /api/credentials/:id/decrypt - 解密凭证');
  console.log('  POST /api/credentials           - 创建凭证');
  console.log('  DELETE /api/credentials/:id     - 删除凭证');
});
