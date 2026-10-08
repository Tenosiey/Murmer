<!--
  Full-screen overlay for connection lifecycle states. The connecting state
  fades in after a short delay so fast connects never flash the overlay;
  disconnected/failed states offer retry and a way back to the server list.
  While the page is reconnecting by itself the overlay says so, so "Try
  again" reads as "now" rather than as the only way back.
-->
<script lang="ts">
  import { t } from '$lib/i18n';
  interface Props {
    state: 'connecting' | 'disconnected' | 'failed';
    server?: string | null;
    /** The page is retrying on its own after losing an established connection. */
    reconnecting?: boolean;
    onRetry: () => void;
    onBack: () => void;
  }

  let {
    state,
    server = null,
    reconnecting = false,
    onRetry,
    onBack
  }: Props = $props();
</script>

<div class="connection-overlay" class:connecting={state === 'connecting'} role="alert">
  <div class="connection-card">
    {#if state === 'connecting'}
      <div class="spinner" aria-hidden="true"></div>
      <h2>{t('connectionOverlay.connecting')}</h2>
      <p class="detail">{server ?? t('connectionOverlay.unknownServer')}</p>
    {:else}
      <h2>
        {t(
          reconnecting
            ? 'connectionOverlay.reconnecting'
            : state === 'failed'
              ? 'connectionOverlay.failed'
              : 'connectionOverlay.lost'
        )}
      </h2>
      <p>
        {t(
          reconnecting
            ? 'connectionOverlay.reconnectingDetail'
            : state === 'failed'
              ? 'connectionOverlay.failedDetail'
              : 'connectionOverlay.lostDetail'
        )}
      </p>
      <p class="detail">{server ?? t('connectionOverlay.unknownServer')}</p>
      <div class="actions">
        <button type="button" class="btn btn-primary" onclick={onRetry}>{t('connectionOverlay.tryAgain')}</button>
        <button type="button" class="btn" onclick={onBack}>{t('connectionOverlay.backToServers')}</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .connection-overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-5);
    background: var(--color-overlay);
    backdrop-filter: blur(6px);
  }

  .connection-overlay.connecting {
    animation: connection-fade-in 0.2s ease 0.4s both;
  }

  @keyframes connection-fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .connection-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    max-width: 420px;
    padding: var(--space-6);
    border-radius: var(--radius-lg);
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-lg);
    text-align: center;
  }

  h2 {
    font-size: var(--text-xl);
  }

  p {
    margin: 0;
    color: var(--color-muted);
    line-height: 1.5;
  }

  .detail {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    word-break: break-all;
  }

  .spinner {
    width: var(--space-6);
    height: var(--space-6);
    border-radius: 50%;
    border: 3px solid var(--color-surface-raised);
    border-top-color: var(--color-primary);
    animation: connection-spin 0.8s linear infinite;
  }

  @keyframes connection-spin {
    to {
      transform: rotate(360deg);
    }
  }

  .actions {
    display: flex;
    gap: var(--space-3);
    margin-top: var(--space-2);
    flex-wrap: wrap;
    justify-content: center;
  }
</style>
