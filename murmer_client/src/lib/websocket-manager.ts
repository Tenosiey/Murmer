/**
 * WebSocket connection management with automatic reconnection and message handling.
 */

import type { Message } from './types';

/** Callback type for message handlers */
type MessageHandler = (msg: Message) => void;

/** Callback type for connection open event */
type OpenCallback = () => void;

/** Details about why and how a connection closed. */
export interface CloseInfo {
  /** Whether the connection had been successfully opened before closing. */
  opened: boolean;
  /** Whether the close was requested locally via disconnect(). */
  intentional: boolean;
}

/** Abort connection attempts that have not opened within this window. */
const CONNECT_TIMEOUT_MS = 10_000;

/**
 * Manages a WebSocket connection with event handlers and lifecycle management.
 */
export class WebSocketManager {
  private socket: WebSocket | null = null;
  private currentUrl: string | null = null;
  private handlers: Record<string, MessageHandler[]> = {};
  /** Sockets closed locally via disconnect(), so their close events can be told apart. */
  private intentionallyClosed = new WeakSet<WebSocket>();

  /**
   * Connect to a WebSocket URL.
   * @param url - WebSocket URL to connect to
   * @param onOpen - Optional callback when connection opens
   * @param onMessage - Callback to handle incoming messages
   * @param onClose - Optional callback when connection closes
   * @param onError - Optional callback when an error occurs
   */
  connect(
    url: string,
    onMessage: (msg: Message) => void,
    onOpen?: OpenCallback,
    onClose?: (info: CloseInfo) => void,
    onError?: (error: Event) => void
  ): void {
    // Close existing connection if URL changed
    if (this.socket && this.currentUrl === url) {
      return;
    }

    this.disconnect();
    this.currentUrl = url;

    if (import.meta.env.DEV) console.log('Connecting to WebSocket', url);

    this.socket = new WebSocket(url);
    const socket = this.socket;
    let opened = false;

    // Browsers can take a long time to give up on unreachable hosts;
    // enforce our own deadline so the user gets timely feedback.
    const connectTimer = setTimeout(() => {
      if (!opened) {
        if (import.meta.env.DEV) console.warn('WebSocket connect timed out');
        socket.close();
      }
    }, CONNECT_TIMEOUT_MS);

    this.socket.addEventListener('open', () => {
      if (import.meta.env.DEV) console.log('WebSocket connection opened');
      opened = true;
      clearTimeout(connectTimer);
      onOpen?.();
    });

    this.socket.addEventListener('message', (ev) => {
      if (import.meta.env.DEV) console.log('Received:', ev.data);
      try {
        const msg: Message = JSON.parse(ev.data);
        onMessage(msg);

        // Trigger registered handlers
        if (msg.type && this.handlers[msg.type]) {
          for (const handler of this.handlers[msg.type]) {
            handler(msg);
          }
        }
      } catch (error) {
        console.error('Failed to parse WebSocket message:', error);
      }
    });

    this.socket.addEventListener('close', () => {
      if (import.meta.env.DEV) console.log('WebSocket connection closed');
      clearTimeout(connectTimer);
      const intentional = this.intentionallyClosed.has(socket);
      // Only clear state if a newer connection hasn't replaced this socket.
      if (this.socket === socket) {
        this.socket = null;
        this.currentUrl = null;
      }
      onClose?.({ opened, intentional });
    });

    this.socket.addEventListener('error', (e) => {
      if (import.meta.env.DEV) console.error('WebSocket error', e);
      onError?.(e);
    });
  }

  /**
   * Send a JSON message over the WebSocket connection.
   * @param data - Data to serialize and send
   * @returns True if sent successfully, false otherwise
   */
  send(data: any): boolean {
    if (this.socket && this.socket.readyState === WebSocket.OPEN) {
      if (import.meta.env.DEV) console.log('Sending:', data);
      this.socket.send(JSON.stringify(data));
      return true;
    }
    return false;
  }

  /**
   * Check if the WebSocket is currently connected.
   */
  isConnected(): boolean {
    return this.socket !== null && this.socket.readyState === WebSocket.OPEN;
  }

  /**
   * Register a handler for a specific message type.
   * @param type - Message type to listen for
   * @param callback - Handler function
   */
  on(type: string, callback: MessageHandler): void {
    if (!this.handlers[type]) {
      this.handlers[type] = [];
    }
    this.handlers[type].push(callback);
  }

  /**
   * Unregister a handler for a specific message type.
   * @param type - Message type
   * @param callback - Optional specific callback to remove (removes all if omitted)
   */
  off(type: string, callback?: MessageHandler): void {
    if (!this.handlers[type]) return;
    if (callback) {
      this.handlers[type] = this.handlers[type].filter((h) => h !== callback);
      if (this.handlers[type].length === 0) {
        delete this.handlers[type];
      }
    } else {
      delete this.handlers[type];
    }
  }

  /**
   * Disconnect and clean up the WebSocket connection.
   */
  disconnect(): void {
    if (this.socket) {
      this.intentionallyClosed.add(this.socket);
      this.socket.close();
      this.socket = null;
      this.currentUrl = null;
    }
  }
}

