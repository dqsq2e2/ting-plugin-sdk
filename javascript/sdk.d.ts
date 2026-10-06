export declare const SDK_VERSION: "2.0.3";
export declare const MAX_CHUNK_BYTES = 262144;
export type ResourceId = string;
export type ChunkRef = string;
export type SessionId = string;
export type Result<T> = { ok: true; data: T } | {
  ok: false; error: { code: string; message: string; details: unknown;
    plugin_id: string; capability_id: string; operation: string;
    request_id: string; retryable: boolean };
};
export declare function host<T = unknown>(method: string, input?: object): Promise<T>;
export declare const resources: Readonly<{
  stat(resource: ResourceId): Promise<{
    length: number | null; mime_type: string | null; readable: boolean;
    writable: boolean; seekable: boolean; revision: string | null; finished: boolean;
  }>;
  readAt(resource: ResourceId, offset: number, max_bytes?: number): Promise<{
    bytes: Uint8Array; eof: boolean;
  }>;
  writeAt(resource: ResourceId, offset: number, bytes: Uint8Array): number;
  createOutput(mime_type?: string | null): Promise<{ resource: ResourceId }>;
  finish(resource: ResourceId): Promise<unknown>;
  close(resource: ResourceId): Promise<unknown>;
  createSession(): Promise<{ session_id: SessionId }>;
  closeSession(session_id: SessionId): Promise<unknown>;
}>;
export declare function readResource(resource: ResourceId, length: number, max_bytes?: number): Promise<Uint8Array>;
export declare function encodeBase64(bytes: Uint8Array): string;
export declare function decodeBase64(text: string, max_bytes?: number): Uint8Array;
export declare function createOutputFromBytes(bytes: Uint8Array, mime_type: string): Promise<ResourceId>;
export interface ScraperResult {
  id: string | null;
  source_url: string | null;
  title: string;
  author: string | null;
  narrator: string | null;
  cover_url: string | null;
  intro: string | null;
  subtitle: string | null;
  publisher: string | null;
  language: string | null;
  genre: string | null;
  published_year: number | null;
  published_date: string | null;
  isbn: string | null;
  asin: string | null;
  explicit: boolean | null;
  abridged: boolean | null;
  tags: string[];
  duration: number | null;
  score: number | null;
  chapter_title_template: string | null;
  chapter_titles: string[];
}
export interface SearchPage {
  items: ScraperResult[];
  page: number;
  page_size: number;
  total: number | null;
  has_more: boolean | null;
}
export declare function publishSearch(
  parsed: { items: object[]; total?: number | null; has_more?: boolean | null },
  request: { page: number; page_size: number },
): SearchPage;
export declare function success<T>(data: T): Result<T>;
export declare function failure(code: string, message: string, context: {
  plugin_id: string; capability_id: string; operation: string; request_id: string;
}): Result<never>;
