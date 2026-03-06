<!--
  Status indicator dot for server availability with smooth animations.
  Uses Material 3 semantic colors for clear status communication.
-->
<script lang="ts">
  export let online: boolean | null = null;
  export let size: 'sm' | 'md' | 'lg' = 'md';
  export let pulse = true;
  
  $: label = online === null ? 'Checking status' : online ? 'Online' : 'Offline';
  $: statusClass = online === null ? 'checking' : online ? 'online' : 'offline';
  
  const sizeMap = {
    sm: '0.5rem',
    md: '0.65rem',
    lg: '0.8rem'
  };
</script>

<span
  class="status status-{statusClass} status-{size}"
  class:status-pulse={pulse && online === true}
  style="width: {sizeMap[size]}; height: {sizeMap[size]}"
  aria-label={label}
  title={label}
  role="status"
></span>

<style>
  .status {
    border-radius: 50%;
    display: inline-block;
    position: relative;
    transition: all var(--motion-duration-short) var(--motion-easing-standard);
  }

  .status-online {
    background: var(--md-sys-color-success);
    box-shadow: 0 0 4px color-mix(in srgb, var(--md-sys-color-success) 60%, transparent);
  }

  .status-offline {
    background: var(--md-sys-color-error);
    opacity: 0.7;
  }

  .status-checking {
    background: var(--md-sys-color-on-surface-variant);
    opacity: 0.5;
    animation: status-checking 1.5s ease-in-out infinite;
  }

  .status-pulse::after {
    content: '';
    position: absolute;
    inset: -2px;
    border-radius: 50%;
    background: var(--md-sys-color-success);
    opacity: 0.6;
    animation: status-pulse 2.5s cubic-bezier(0.4, 0, 0.6, 1) infinite;
  }

  @keyframes status-pulse {
    0%, 100% {
      opacity: 0.6;
      transform: scale(1);
    }
    50% {
      opacity: 0;
      transform: scale(2);
    }
  }

  @keyframes status-checking {
    0%, 100% {
      opacity: 0.3;
    }
    50% {
      opacity: 0.6;
    }
  }
</style>
