declare module "buffer" {
  export interface Buffer extends Uint8Array {
    toString(encoding?: string): string;
  }
  /** Not `Uint8Array`'s static side: `from` takes an encoding, not a map function. */
  export interface BufferConstructor {
    new (size: number): Buffer;
    readonly prototype: Buffer;
    /** Encodings: utf8 (default), hex, base64, base64url, latin1. */
    from(value: string | ArrayBuffer | ArrayLike<number>, encoding?: string): Buffer;
    alloc(size: number): Buffer;
    isBuffer(value: unknown): value is Buffer;
    byteLength(value: string, encoding?: string): number;
    concat(list: Uint8Array[]): Buffer;
  }
  export const Buffer: BufferConstructor;
}
declare module "path" {
  export function join(...parts: string[]): string;
  /** Against `/`: a plugin has no working directory. */
  export function resolve(...parts: string[]): string;
  export function normalize(path: string): string;
  export function dirname(path: string): string;
  export function basename(path: string, suffix?: string): string;
  export function extname(path: string): string;
  export function isAbsolute(path: string): boolean;
  export const sep: "/";
  const path: {
    join: typeof join;
    resolve: typeof resolve;
    normalize: typeof normalize;
    dirname: typeof dirname;
    basename: typeof basename;
    extname: typeof extname;
    isAbsolute: typeof isAbsolute;
    sep: "/";
  };
  export default path;
}
declare module "url" {
  export class URL {
    constructor(url: string, base?: string);
    readonly href: string;
    readonly protocol: string;
    readonly host: string;
    readonly hostname: string;
    readonly port: string;
    readonly pathname: string;
    readonly search: string;
    readonly hash: string;
    readonly origin: string;
    readonly username: string;
    readonly password: string;
    toString(): string;
    toJSON(): string;
  }
  export class URLSearchParams {
    constructor(init?: string | Record<string, string> | [string, string][]);
    append(name: string, value: string): void;
    delete(name: string): void;
    get(name: string): string | null;
    getAll(name: string): string[];
    has(name: string): boolean;
    set(name: string, value: string): void;
    forEach(f: (value: string, name: string, params: URLSearchParams) => void): void;
    entries(): IterableIterator<[string, string]>;
    [Symbol.iterator](): IterableIterator<[string, string]>;
    toString(): string;
  }
}
declare module "crypto" {
  export interface Hash {
    update(data: string | Uint8Array): Hash;
    digest(): import("buffer").Buffer;
    digest(encoding: "hex" | "base64" | "base64url"): string;
  }
  /** md5, sha1, sha256 or sha512. */
  export function createHash(algorithm: string): Hash;
  /** At most 65536 bytes. */
  export function randomBytes(size: number): import("buffer").Buffer;
  export function randomUUID(): string;
}
declare module "zlib" {
  export function deflateSync(data: string | Uint8Array): import("buffer").Buffer;
  /** Throws past 64 MiB of output. */
  export function inflateSync(data: Uint8Array): import("buffer").Buffer;
  export function inflateRawSync(data: Uint8Array): import("buffer").Buffer;
  export function gzipSync(data: string | Uint8Array): import("buffer").Buffer;
  export function gunzipSync(data: Uint8Array): import("buffer").Buffer;
}
declare module "console" {
  const console: Console;
  export default console;
}
declare module "os" {
  export function platform(): string;
  export function arch(): string;
  export const EOL: string;
  const os: { platform: typeof platform; arch: typeof arch; EOL: string };
  export default os;
}
declare module "process" {
  export interface CommandOutput {
    code: number;
    stdout: string;
    stderr: string;
  }
  export interface ProcessOptions {
    /** Set for the child; only names in `capabilities.process.env`. */
    env?: Record<string, string>;
    /** Absolute, inside a granted `capabilities.fs.read` root. */
    cwd?: string;
  }
  export interface SpawnOptions extends ProcessOptions {
    stdout?: "pipe" | "ignore";
    stderr?: "pipe" | "ignore";
  }
  /** `code` for a normal exit, `signal` when a signal ended the child. */
  export interface ExitStatus {
    code: number | null;
    signal: number | null;
  }
  /**
   * A running child, leader of its own process group. When this handle is dropped or
   * garbage collected the group gets SIGTERM, then SIGKILL 2 s later. Keep the handle
   * (for example on `globalThis`) while the child should live, or use `run`.
   */
  export interface ChildProcess {
    readonly pid: number;
    /** The next stdout line without its newline; null at EOF or when ignored. */
    readLine(): Promise<string | null>;
    /** The next stderr line; null at EOF or when ignored. */
    readErrLine(): Promise<string | null>;
    /** At most 1 MiB per call. */
    write(data: string | Uint8Array): Promise<void>;
    /** Closes stdin after the writes before it. */
    closeStdin(): Promise<void>;
    wait(): Promise<ExitStatus>;
    /** Signals the process group; SIGTERM by default. */
    kill(signal?: "SIGTERM" | "SIGKILL" | "SIGINT" | "SIGHUP"): void;
  }
  /** Runs a granted command; each stream is bounded to 8 MiB. */
  export function run(command: string, args?: string[], options?: ProcessOptions): Promise<CommandOutput>;
  export function spawn(command: string, args?: string[], options?: SpawnOptions): ChildProcess;
  /** Quits the whole shell; needs `capabilities.process.exit`. */
  export function exit(): void;
  export function nextTick(callback: (...args: unknown[]) => void, ...args: unknown[]): void;
  export const platform: string;
  export const arch: string;
  const process: {
    run: typeof run;
    spawn: typeof spawn;
    exit: typeof exit;
    nextTick: typeof nextTick;
    platform: string;
    arch: string;
  };
  export default process;
}
declare module "fs/promises" {
  export interface Dirent {
    name: string;
    isDirectory(): boolean;
    isFile(): boolean;
  }
  export interface MakeDirectoryOptions {
    recursive?: boolean;
  }
  /** At most 64 MiB. */
  export function readFile(path: string): Promise<Uint8Array>;
  export function readFile(path: string, encoding: "utf8" | { encoding: "utf8" }): Promise<string>;
  /** At most 8 MiB. */
  export function writeFile(path: string, contents: string | Uint8Array): Promise<void>;
  export function readdir(path: string): Promise<string[]>;
  export function readdir(path: string, options: { withFileTypes: true }): Promise<Dirent[]>;
  export function exists(path: string): Promise<boolean>;
  export function unlink(path: string): Promise<void>;
  export function rmdir(path: string): Promise<void>;
  export function mkdir(path: string, options?: MakeDirectoryOptions): Promise<void>;
  /** An open file, closed by close() or when its runtime goes away. Await each call before the next: they are not ordered. */
  export interface FileHandle {
    /** Up to maxBytes (64 KiB by default, at most 1 MiB); null at EOF. */
    read(maxBytes?: number): Promise<Uint8Array | null>;
    /** At most 1 MiB per call. */
    write(data: string | Uint8Array): Promise<void>;
    close(): Promise<void>;
  }
  /** "r" (default) needs fs.read; "w" truncates, "a" appends, both need fs.write; "r+" both. */
  export function open(path: string, flags?: "r" | "w" | "a" | "r+"): Promise<FileHandle>;
}
declare module "net" {
  export interface Socket {
    /** At most 1 MiB per call. */
    write(data: string | Uint8Array): Promise<void>;
    /** Raw bytes, no idle timeout; null at EOF or after close(). */
    read(maxBytes?: number): Promise<Uint8Array | null>;
    close(): void;
  }
  export function connect(host: string, port: number): Promise<Socket>;
  /** A unix socket granted by its exact path in `capabilities.network.unix`. */
  export function connect(target: { path: string }): Promise<Socket>;
  const net: { connect: typeof connect };
  export default net;
}
declare module "websocket" {
  export interface WebSocketSocket {
    /** The next text or binary message. */
    read(): Promise<string | Uint8Array>;
    write(data: string | Uint8Array): Promise<void>;
    /** Sends and flushes a close frame. */
    close(): Promise<void>;
  }
  export interface WebSocketConnectOptions {
    /** Credential and WebSocket control headers are refused. */
    headers?: Readonly<Record<string, string>>;
  }
  /** Client sockets to granted hosts; not the browser constructor. */
  export const WebSocket: {
    connect(url: string, options?: WebSocketConnectOptions): Promise<WebSocketSocket>;
  };
}
interface Console {
  debug(...values: unknown[]): void;
  log(...values: unknown[]): void;
  info(...values: unknown[]): void;
  warn(...values: unknown[]): void;
  error(...values: unknown[]): void;
}
declare var console: Console;
declare const process: typeof import("process").default;
type Buffer = import("buffer").Buffer;
declare const Buffer: typeof import("buffer").Buffer;
interface FetchResponse {
  readonly status: number;
  readonly ok: boolean;
  readonly url: string;
  /** Lowercase names; repeated headers joined with ", ". */
  readonly headers: Readonly<Record<string, string>>;
  /** UTF-8, invalid bytes replaced. */
  text(): Promise<string>;
  json(): Promise<unknown>;
  arrayBuffer(): Promise<ArrayBuffer>;
}
/** `fetch(url, { stream: true })`: the body unread, without a size limit. */
interface FetchStream {
  readonly status: number;
  readonly ok: boolean;
  readonly url: string;
  readonly headers: Readonly<Record<string, string>>;
  /** The next chunk; null at the end of the body or after close(). */
  read(): Promise<Uint8Array | null>;
  close(): void;
}
interface FetchOptions {
  /** GET by default; what may reach a host and path is `capabilities.network`. */
  method?: string;
  /** Host, Origin, Content-Length and the like are refused. */
  headers?: Record<string, string>;
  body?: string | Uint8Array;
  /** Milliseconds, 1 to 300000, 30000 by default; bounds the whole request including redirects and, unless `stream`, reading the body; a stream has no read timeout. */
  timeout?: number;
  /** Resolve once the headers arrive and read the body chunk by chunk. */
  stream?: boolean;
  /**
   * HTTP/1.1 over this unix socket, granted in `capabilities.network.unix`. The URL's
   * host only fills Host; redirects are not followed.
   */
  unix?: string;
}
/** Every redirect is authorized again; bodies are at most 8 MiB unless streamed. */
declare function fetch(url: string, options: FetchOptions & { stream: true }): Promise<FetchStream>;
declare function fetch(url: string, options?: FetchOptions): Promise<FetchResponse>;
/** Web Storage; values are strings. */
interface Storage {
  readonly length: number;
  key(index: number): string | null;
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
  clear(): void;
  /** Resolves once the last change is on disk; retries a failed write. */
  flush(): Promise<void>;
}
/** A JSON file per plugin; needs `capabilities.storage`. */
declare var localStorage: Storage;
/** In memory, until the shell exits. */
declare var sessionStorage: Storage;
declare function setTimeout(handler: (...args: unknown[]) => void, ms?: number, ...args: unknown[]): number;
declare function setInterval(handler: (...args: unknown[]) => void, ms?: number, ...args: unknown[]): number;
declare function clearTimeout(id?: number): void;
declare function clearInterval(id?: number): void;
declare function queueMicrotask(callback: () => void): void;
// As @types/node does: lib.dom's URL wins when it is loaded, so neither clashes
type URL_ = import("url").URL;
type URLSearchParams_ = import("url").URLSearchParams;
interface URL extends URL_ {}
interface URLSearchParams extends URLSearchParams_ {}
declare var URL: typeof globalThis extends { onmessage: any; URL: infer T } ? T : typeof import("url").URL;
declare var URLSearchParams: typeof globalThis extends { onmessage: any; URLSearchParams: infer T }
  ? T
  : typeof import("url").URLSearchParams;
