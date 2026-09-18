import { type SubmitEvent, useEffect, useState } from 'react';
import { type AcmeStatus, verifyAcmeToken } from '../lib/api';
import {
  getErrorMessage,
  useAcmeSettingsQuery,
  useAcmeStatusQuery,
  useSaveAcmeSettingsMutation,
} from '../lib/query';

/** What the daemon reports about the certificate, in one line. */
function statusLine(status: AcmeStatus): { tone: 'success' | 'warning' | 'danger'; text: string } {
  const { certificate } = status;

  if (!status.enabled) {
    return {
      tone: 'warning',
      text: 'Automatic certificates are off, so nothing is requested.',
    };
  }

  if (!certificate.exists) {
    return {
      tone: 'warning',
      text: status.request_file.written
        ? 'Requested from the proxy. No certificate has been issued yet, so environment and per-deployment hostnames are served over HTTP until one is. The next deploy puts it in front of them.'
        : 'Enabled, but the request is not in place yet. Saving these settings writes it.',
    };
  }

  switch (certificate.lifecycle) {
    case 'ok':
      return {
        tone: 'success',
        text: `Issued, and it expires in ${certificate.days_remaining} day(s) at ${certificate.expires_at}. Environment and per-deployment hostnames are served over HTTPS.`,
      };
    case 'expiring':
      return {
        tone: 'warning',
        text: `Issued but close to expiry (${certificate.days_remaining} day(s) left). Angie renews it before it lapses.`,
      };
    case 'expired':
      return {
        tone: 'danger',
        text: 'The certificate has expired. Angie requests a new one on its next reload.',
      };
    default:
      return {
        tone: 'danger',
        text: `A certificate file exists at ${certificate.path} but could not be read.`,
      };
  }
}

const DEFAULT_DIRECTORY = 'https://acme-v02.api.letsencrypt.org/directory';
const STAGING_DIRECTORY = 'https://acme-staging-v02.api.letsencrypt.org/directory';

function tokenSourceLabel(source: string | null | undefined): string {
  switch (source) {
    case 'environment':
      return 'the DEKU_ACME_API_TOKEN environment variable';
    case 'inline':
      return 'the daemon config';
    case 'file':
      return 'a file on the host';
    default:
      return 'this host';
  }
}

export default function AcmeSettingsPanel() {
  const settingsQuery = useAcmeSettingsQuery();
  const statusQuery = useAcmeStatusQuery();
  const saveMutation = useSaveAcmeSettingsMutation();

  const [enabled, setEnabled] = useState(false);
  const [directory, setDirectory] = useState(DEFAULT_DIRECTORY);
  const [wildcard, setWildcard] = useState(false);
  const [token, setToken] = useState('');
  const [busy, setBusy] = useState<'save' | 'verify' | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const settings = settingsQuery.data;

  // Seed the form from the server once, so a reload shows what is configured.
  useEffect(() => {
    if (!settings) return;
    setEnabled(settings.enabled);
    setDirectory(settings.directory || DEFAULT_DIRECTORY);
    setWildcard(settings.wildcard);
  }, [settings]);

  async function handleVerify() {
    setBusy('verify');
    setError(null);
    setNotice(null);
    try {
      const result = await verifyAcmeToken(token.trim() || undefined);
      setNotice(`Token accepted, and it can see zone ${result.zone_id}.`);
    } catch (nextError) {
      setError(getErrorMessage(nextError, 'The token could not be verified.'));
    } finally {
      setBusy(null);
    }
  }

  async function handleSave(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy('save');
    setError(null);
    setNotice(null);
    try {
      await saveMutation.mutateAsync({
        enabled,
        directory: directory.trim(),
        provider: 'cloudflare',
        wildcard,
        // Only sent when the operator typed a new one, so an untouched field
        // does not clear the stored token.
        api_token: token.trim() || undefined,
      });
      setToken('');
      setNotice(
        enabled
          ? 'Saved. Certificates will be requested for this host.'
          : 'Saved. No certificates will be requested.'
      );
    } catch (nextError) {
      setError(getErrorMessage(nextError, 'Unable to save these settings.'));
    } finally {
      setBusy(null);
    }
  }

  return (
    <article className="panel stack-md">
      <h2 className="section-title">Automatic certificates</h2>
      <p className="text-muted">
        Issue and renew certificates automatically. A wildcard certificate covers every app's
        environment and per-deployment hostnames, which needs a DNS provider token so the
        certificate authority can verify the domain.
      </p>

      {/* What was asked for, and what happened: the two differ while a
          certificate is being issued, which is the state worth showing. */}
      {statusQuery.data ? (
        <p className={`callout callout-${statusLine(statusQuery.data).tone}`}>
          {statusLine(statusQuery.data).text}
        </p>
      ) : null}

      {settingsQuery.isPending ? <p className="text-muted">Loading settings…</p> : null}

      {notice ? <p className="callout callout-success">{notice}</p> : null}
      {error ? <p className="callout callout-danger">{error}</p> : null}

      <form onSubmit={handleSave} className="stack-md" noValidate>
        <label className="checkbox-field" htmlFor="acme-enabled">
          <input
            id="acme-enabled"
            className="checkbox-input"
            type="checkbox"
            checked={enabled}
            onChange={(event) => setEnabled(event.target.checked)}
            disabled={busy !== null}
          />
          <span>
            Issue certificates automatically
            <small>Off means nothing is requested from a certificate authority.</small>
          </span>
        </label>

        <label className="checkbox-field" htmlFor="acme-wildcard">
          <input
            id="acme-wildcard"
            className="checkbox-input"
            type="checkbox"
            checked={wildcard}
            onChange={(event) => setWildcard(event.target.checked)}
            disabled={busy !== null}
          />
          <span>
            Cover environment and preview hostnames
            <small>
              Requests a wildcard for the host's global domain, so staging and per-deployment URLs
              get HTTPS too. Requires the global domain to be set.
            </small>
          </span>
        </label>

        <div className="form-group">
          <label className="form-label" htmlFor="acme-token">
            {wildcard ? 'DNS provider token' : 'DNS provider token (needed for a wildcard)'}
          </label>
          <input
            id="acme-token"
            className="input"
            type="password"
            value={token}
            onChange={(event) => {
              setToken(event.target.value);
              if (error) setError(null);
            }}
            placeholder={
              settings?.token_configured
                ? 'Stored — enter a new token to replace it'
                : 'Paste a token'
            }
            disabled={busy !== null}
            autoComplete="off"
            spellCheck={false}
          />
          <p className="text-muted">
            {settings?.token_configured
              ? `A token is already configured from ${tokenSourceLabel(settings.token_source)}. Saving writes a new one to a 0600 file on the host and records only its path.`
              : 'A Cloudflare API token scoped to Zone:Read and DNS:Edit. It is written to a 0600 file on the host; the config records only its path.'}
          </p>
        </div>

        <div className="form-group">
          <label className="form-label" htmlFor="acme-directory">
            ACME directory
          </label>
          <input
            id="acme-directory"
            className="input"
            value={directory}
            onChange={(event) => setDirectory(event.target.value)}
            placeholder={DEFAULT_DIRECTORY}
            disabled={busy !== null}
            spellCheck={false}
          />
          <p className="text-muted">
            {settings?.account_email
              ? `The account contact is ${settings.account_email}, from the Let's Encrypt account email above.`
              : "Set the Let's Encrypt account email above to register a contact for expiry notices."}
          </p>
        </div>

        <p className="text-muted">
          Use the staging directory while testing so a mistake does not use up real rate limits:{' '}
          <button
            type="button"
            className="btn btn-sm btn-outline"
            onClick={() => setDirectory(STAGING_DIRECTORY)}
            disabled={busy !== null}
          >
            Use staging
          </button>
        </p>

        <div className="form-actions">
          <button
            className="btn btn-outline"
            type="button"
            onClick={() => {
              void handleVerify();
            }}
            disabled={busy !== null}
          >
            {busy === 'verify' ? <span className="loading-spinner" /> : null}
            <span>Test token</span>
          </button>
          <button className="btn btn-primary" type="submit" disabled={busy !== null}>
            {busy === 'save' ? <span className="loading-spinner" /> : null}
            <span>Save settings</span>
          </button>
        </div>
      </form>
    </article>
  );
}
