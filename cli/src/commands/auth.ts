import type { CliConfig } from '../types/cli.js';
import { printResult } from '../output/print.js';
import { createSdk, parseOptions, requireArg } from './common.js';
import { saveConfig } from '../config/store.js';

function bearerFromConfig(config: CliConfig): string | undefined {
  return config.token ?? config.sessionToken;
}

export async function runAuth(config: CliConfig, argv: string[]): Promise<void> {
  const [subcommand, ...rest] = argv;
  const sdk = createSdk(config);
  const options = parseOptions(rest);

  switch (subcommand) {
    case 'login': {
      const url = requireArg(options, 'url');
      const token = requireArg(options, 'token');
      const next = { ...config, baseUrl: url, token };
      saveConfig(next);
      printResult({ ok: true, baseUrl: url, tokenStored: true }, config.output);
      return;
    }
    case 'status':
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
          tokenPreserved: Boolean(config.token),
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
      saveConfig({ ...config, sessionToken: session.session.sessionToken });
      printResult(session, config.output);
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
          .map((scope) => scope.trim())
          .filter(Boolean);
        const ttlRaw = options['ttl-seconds'];
        const ttlSeconds = typeof ttlRaw === 'string' ? Number(ttlRaw) : undefined;
        if (!config.sessionToken) {
          throw new Error(
            'No session token found. Run `toani auth session --privy-access-token ...` first.'
          );
        }
        const result = (await sdk.client.post(
          '/auth/access-token',
          {
            scopes,
            ttl_seconds: ttlSeconds,
          },
          {
            headers: {
              Authorization: `Bearer ${config.sessionToken}`,
            },
          }
        )) as { access_token: string; [key: string]: unknown };

        const shouldStore = options.store !== false;
        if (shouldStore) {
          saveConfig({ ...config, token: result.access_token });
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
        const revokedResp = await sdk.client.post(`/tokens/${tokenId}/revoke`, {});
        const revoked = Boolean((revokedResp as { revoked?: boolean }).revoked);
        printResult({ revoked, tokenId }, config.output);
        return;
      }
      throw new Error(
        'Usage: toani auth access-token <create|revoke> [options]'
      );
    }
    default:
      throw new Error(
        'Usage: toani auth <login|status|logout|session|me|memberships|access-token> [options]'
      );
  }
}
