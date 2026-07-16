<!--
  Hue/saturation color wheel, modeled on Zen Browser's theme picker: the
  angle around the wheel (clockwise from the top) picks the hue, the
  distance from the center picks the saturation. The wheel itself is pure
  CSS — a conic hue gradient under a radial white fade — so the literal
  colors below are the wheel's content, not UI chrome.
-->
<script lang="ts">
  interface Props {
    hue: number;
    saturation: number;
    onchange: (hue: number, saturation: number) => void;
  }

  let { hue, saturation, onchange }: Props = $props();

  let wheel: HTMLDivElement | undefined = $state();
  let dragging = false;

  let rad = $derived((hue * Math.PI) / 180);
  let dotLeft = $derived(50 + (Math.sin(rad) * saturation) / 2);
  let dotTop = $derived(50 - (Math.cos(rad) * saturation) / 2);

  function pick(event: PointerEvent) {
    if (!wheel) return;
    const rect = wheel.getBoundingClientRect();
    const dx = event.clientX - (rect.left + rect.width / 2);
    const dy = event.clientY - (rect.top + rect.height / 2);
    const h = (Math.atan2(dx, -dy) * (180 / Math.PI) + 360) % 360;
    const s = Math.min(1, Math.hypot(dx, dy) / (rect.width / 2)) * 100;
    onchange(Math.round(h), Math.round(s));
  }

  function handlePointerDown(event: PointerEvent) {
    dragging = true;
    wheel?.setPointerCapture(event.pointerId);
    pick(event);
  }

  function handlePointerMove(event: PointerEvent) {
    if (dragging) pick(event);
  }

  function handlePointerUp() {
    dragging = false;
  }

  function handleKeydown(event: KeyboardEvent) {
    const step = event.shiftKey ? 15 : 5;
    let h = hue;
    let s = saturation;
    switch (event.key) {
      case 'ArrowLeft':
        h = (h - step + 360) % 360;
        break;
      case 'ArrowRight':
        h = (h + step) % 360;
        break;
      case 'ArrowUp':
        s = Math.min(100, s + step);
        break;
      case 'ArrowDown':
        s = Math.max(0, s - step);
        break;
      default:
        return;
    }
    event.preventDefault();
    onchange(h, s);
  }
</script>

<div
  class="wheel"
  bind:this={wheel}
  role="slider"
  tabindex="0"
  aria-label="Theme color wheel"
  aria-valuemin={0}
  aria-valuemax={360}
  aria-valuenow={Math.round(hue)}
  aria-valuetext={`Hue ${Math.round(hue)} degrees, intensity ${Math.round(saturation)} percent`}
  onpointerdown={handlePointerDown}
  onpointermove={handlePointerMove}
  onpointerup={handlePointerUp}
  onkeydown={handleKeydown}
>
  <div
    class="dot"
    style={`left: ${dotLeft}%; top: ${dotTop}%; background: hsl(${hue} ${saturation}% 55%);`}
  ></div>
</div>

<style>
  .wheel {
    position: relative;
    width: 10.5rem;
    aspect-ratio: 1;
    flex-shrink: 0;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-surface-outline);
    cursor: crosshair;
    touch-action: none;
    background:
      radial-gradient(circle closest-side, #ffffff 0%, rgba(255, 255, 255, 0) 72%),
      conic-gradient(
        hsl(0 100% 55%),
        hsl(60 100% 55%),
        hsl(120 100% 55%),
        hsl(180 100% 55%),
        hsl(240 100% 55%),
        hsl(300 100% 55%),
        hsl(360 100% 55%)
      );
  }

  .wheel:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
  }

  .dot {
    position: absolute;
    width: 1.125rem;
    aspect-ratio: 1;
    border-radius: var(--radius-pill);
    transform: translate(-50%, -50%);
    border: 2px solid #ffffff;
    box-shadow:
      0 0 0 1px rgba(0, 0, 0, 0.35),
      var(--shadow-xs);
    pointer-events: none;
  }
</style>
