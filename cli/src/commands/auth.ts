import type { CliConfig } from '../types/cli.js';
import { printResult } from '../output/print.js';
import { createSdk, parseOptions, requireArg } from './common.js';
import { saveConfig } from '../config/store.js';

function automationTokenFromConfig(config: CliConfig): string | undefined {
  return config.automationToken ?? config.token;
}

function bearerFromConfig(config: CliConfig): string | undefined {
  return automationTokenFromConfig(config) ?? config.sessionToken;
}

function sessionAuthorization(config: CliConfig): { Authorization: string } {
  if (!config.sessionToken) {
    throw new Error(
      'No session token found. Run `toani auth session --privy-access-token ...` or `toani auth login --session-token ...` first.'
    );
  }
  return { Authorization: `Bearer ${config.sessionToken}` };
}

export async function runAuth(config: CliConfig, argv: string[]): Promise<void> {
  const [subcommand, ...rest] = argv;
  const sdk = createSdk(config);
  const options = parseOptions(rest);

  switch (subcommand) {
    case 'login': {
      const url = requireArg(options, 'url');
      const sessionToken =
        (options['session-token'] as string | undefined) ?? requireArg(options, 'token');
      const next = { ...config, baseUrl: url, sessionToken };
      saveConfig(next);
      printResult({ ok: true, baseUrl: url, sessionTokenStored: true }, config.output);
      return;
    }
    case 'status': {
      printResult(
        {
          currentProfile: config.currentProfile ?? 'default',
          credentialSource: automationTokenFromConfig(config)
            ? 'automation'
            : config.sessionToken
              ? 'session'
              : 'none',
          tenantId: config.currentTenantId,
          automationTokenPresent: Boolean(automationTokenFromConfig(config)),
          sessionTokenPresent: Boolean(config.sessionToken),
          baseUrl: config.baseUrl,
        },
        config.output
      );
      return;
    }
    case 'me': {
      const bearer = bearerFromConfig(config);
      const me = await sdk.client.get('/auth/me', {
        headers: bearer ? { Authorization: `Bearer ${bearer}` } : undefined,
      });
      printResult(me, config.output);
      return;
    }
    case 'logout': {
      const bearer = bearerFromConfig(config);
      await sdk.client.post(
        '/auth/logout',
        {},
        { headers: bearer ? { Authorization: `Bearer ${bearer}` } : undefined }
      );
      saveConfig({ ...config, sessionToken: undefined });
      printResult(
        {
          ok: true,
          loggedOut: true,
          sessionTokenCleared: true,
          automationTokenPreserved: Boolean(automationTokenFromConfig(config)),
        },
        config.output
      );
      return;
    }
    case 'session': {
      const privyAccessToken = requireArg(options, 'privy-access-token');
      const session = await sdk.auth.createSession({
        privyAccessToken,
        invitationToken: options['invitation-token'] as string | undefined,
      });
      saveConfig({
        ...config,
        sessionToken: session.session.sessionToken,
        currentTenantId: session.currentTenant?.id ?? session.currentMembership?.tenantId,
      });
      printResult(session, config.output);
      return;
    }
    case 'use-tenant': {
      const tenantId = options._[0];
      if (!tenantId) {
        throw new Error('Usage: toani auth use-tenant <tenant-id>');
      }
      saveConfig({ ...config, currentTenantId: tenantId });
      printResult({ ok: true, currentTenantId: tenantId }, config.output);
      return;
    }
    case 'memberships': {
      const bearer = bearerFromConfig(config);
      const memberships = await sdk.client.get('/auth/memberships', {
        headers: bearer ? { Authorization: `Bearer ${bearer}` } : undefined,
      });
      printResult(memberships, config.output);
      return;
    }
    case 'token': {
      const nested = options._[0];

      if (nested === 'create') {
        const name = requireArg(
          options,
          'name',
          'Usage: toani auth token create --name <name> --scope <scope1,scope2> [--ttl-seconds 86400] [--save]'
        );
        const scopes = requireArg(options, 'scope')
          .split(',')
          .map(scope => scope.trim())
          .filter(Boolean);
        const ttlRaw = options['ttl-seconds'];
        const ttlSeconds = typeof ttlRaw === 'string' ? Number(ttlRaw) : undefined;
        const response = await sdk.auth.createAutomationToken(
          {
            name,
            description: options.description as string | undefined,
            scopes,
            ttlSeconds,
            createdVia: 'cli',
          },
          { headers: sessionAuthorization(config) }
        );
        const shouldSave = Boolean(options.save);
        if (shouldSave) {
          saveConfig({
            ...config,
            automationToken: response.tokenValue,
            token: response.tokenValue,
          });
        }
        printResult({ ...response, saved: shouldSave }, config.output);
        return;
      }

      if (nested === 'list') {
        const items = await sdk.auth.listAutomationTokens({
          headers: sessionAuthorization(config),
        });
        printResult(items, config.output);
        return;
      }

      if (nested === 'get') {
        const tokenId = options._[1];
        if (!tokenId) {
          throw new Error('Usage: toani auth token get <token-id>');
        }
        const item = await sdk.auth.getAutomationToken(tokenId, {
          headers: sessionAuthorization(config),
        });
        printResult(item, config.output);
        return;
      }

      if (nested === 'revoke') {
        const tokenId = options._[1] ?? (options['token-id'] as string | undefined);
        if (!tokenId) {
          throw new Error('Usage: toani auth token revoke <token-id>');
        }
        const item = await sdk.auth.revokeAutomationToken(tokenId, {
          headers: sessionAuthorization(config),
        });
        printResult(item, config.output);
        return;
      }

      throw new Error('Usage: toani auth token <create|list|get|revoke> [options]');
    }
    case 'access-token': {
      const nested = options._[0];
      if (nested === 'create') {
        const scopesRaw = options.scope as string | undefined;
        if (!scopesRaw) {
          throw new Error(
            'Usage: toani auth access-token create --scope <scope1,scope2> [--ttl-seconds <seconds>] [--store]'
          );
        }
        const scopes = scopesRaw
          .split(',')
          .map(scope => scope.trim())
          .filter(Boolean);
        const ttlRaw = options['ttl-seconds'];
        const ttlSeconds = typeof ttlRaw === 'string' ? Number(ttlRaw) : undefined;
        const result = (await sdk.client.post(
          '/auth/access-token',
          {
            scopes,
            ttl_seconds: ttlSeconds,
          },
          {
            headers: sessionAuthorization(config),
          }
        )) as { access_token: string; [key: string]: unknown };

        const shouldStore = options.store !== false;
        if (shouldStore) {
          saveConfig({
            ...config,
            automationToken: result.access_token,
            token: result.access_token,
          });
        }

        printResult(
          {
            ...result,
            stored: shouldStore,
          },
          config.output
        );
        return;
      }
      if (nested === 'revoke') {
        const tokenId = requireArg(
          options,
          'token-id',
          'Usage: toani auth access-token revoke --token-id <token-id>'
        );
        const revokedResp = await sdk.client.post(`/tokens/${tokenId}/revoke`, {}, {
          headers: sessionAuthorization(config),
        });
        const revoked = Boolean((revokedResp as { revoked?: boolean }).revoked);
        printResult({ revoked, tokenId }, config.output);
        return;
      }
      throw new Error('Usage: toani auth access-token <create|revoke> [options]');
    }
    default:
      throw new Error(
        'Usage: toani auth <login|status|logout|session|me|memberships|use-tenant|token|access-token> [options]'
      );
  }
}
