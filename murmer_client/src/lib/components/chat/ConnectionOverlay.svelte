<!--
  Full-screen overlay for connection lifecycle states. The connecting state
  fades in after a short delay so fast connects never flash the overlay;
  disconnected/failed states offer retry and a way back to the server list.
-->
<script lang="ts">
  interface Props {
    state: 'connecting' | 'disconnected' | 'failed';
    server?: string | null;
    onRetry: () => void;
    onBack: () => void;
  }

  let {
    state,
    server = null,
    onRetry,
    onBack
  }: Props = $props();
</script>

<div class="connection-overlay" class:connecting={state === 'connecting'} role="alert">
  <div class="connection-card">
    {#if state === 'connecting'}
      <div class="spinner" aria-hidden="true"></div>
      <h2>Connecting…</h2>
      <p class="detail">{server ?? 'Unknown server'}</p>
    {:else}
      <h2>{state === 'failed' ? 'Could not connect' : 'Connection lost'}</h2>
      <p>
        {state === 'failed'
          ? 'The server is offline or unreachable. Check the address or try again later.'
          : 'The connection to the server was lost. It may have gone offline.'}
      </p>
      <p class="detail">{server ?? 'Unknown server'}</p>
      <div class="actions">
        <button type="button" class="btn btn-primary" onclick={onRetry}>Try again</button>
        <button type="button" class="btn" onclick={onBack}>Back to servers</button>
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
