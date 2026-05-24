export type SessionStatus = "active" | "blocked" | "done" | "archived";

export type WorkspaceInfo = {
  root: string;
  git_remote?: string;
  git_branch?: string;
  head?: string;
};

export type Session = {
  id: string;
  title: string;
  status: SessionStatus;
  workspace?: WorkspaceInfo;
  summary?: string;
  created_at: string;
  updated_at: string;
};

export type ArtifactKind =
  | "file"
  | "git_diff"
  | "terminal_output"
  | "stack_trace"
  | "url"
  | "chat_snippet"
  | "tool_invocation"
  | "test_result"
  | "note"
  | "ticket";

export type Artifact = {
  id: string;
  session_id: string;
  kind: ArtifactKind;
  title?: string;
  uri?: string;
  content_hash?: string;
  content_ref?: string;
  body?: string;
  metadata: Record<string, unknown>;
  created_at: string;
};

export type CreateArtifactRequest = {
  kind: ArtifactKind;
  title?: string;
  uri?: string;
  body?: string;
  metadata?: Record<string, unknown>;
  snapshot?: boolean;
};

export type Decision = {
  id: string;
  session_id: string;
  text: string;
  rationale?: string;
  created_at: string;
};

export type AdapterProtocol = "native-http" | "acp" | "mcp" | "stdio" | "filesystem";

export type AdapterCapability =
  | "import_context"
  | "export_context"
  | "stream_updates"
  | "read_workspace"
  | "write_artifact"
  | "tool_calls"
  | "session_resume"
  | "session_observe";

export type CapabilityDescriptor = {
  adapter_id: string;
  protocol: AdapterProtocol;
  version: string;
  capabilities: AdapterCapability[];
  metadata?: Record<string, unknown>;
};

export type PackProfile = "chatgpt" | "claude" | "cursor" | "acp" | "generic";

export type ContextPack = {
  session_id: string;
  profile: PackProfile;
  markdown: string;
  json: unknown;
  created_at: string;
};

export type BusEvent = {
  id: string;
  session_id?: string;
  type: string;
  source: string;
  payload: unknown;
  created_at: string;
};

export class SessionbusClient {
  readonly baseUrl: string;

  constructor(baseUrl = process.env.SESSIONBUS_URL ?? "http://127.0.0.1:8765") {
    this.baseUrl = baseUrl.replace(/\/+$/, "");
  }

  async createSession(input: {
    title: string;
    workspace?: WorkspaceInfo;
    summary?: string;
  }): Promise<Session> {
    return this.request<Session>("/sessions", {
      method: "POST",
      body: JSON.stringify(input),
    });
  }

  async addArtifact(sessionId: string, artifact: CreateArtifactRequest): Promise<Artifact> {
    return this.request<Artifact>(`/sessions/${encodeURIComponent(sessionId)}/artifacts`, {
      method: "POST",
      body: JSON.stringify({ metadata: {}, snapshot: false, ...artifact }),
    });
  }

  async addDecision(
    sessionId: string,
    decision: { text: string; rationale?: string },
  ): Promise<Decision> {
    return this.request<Decision>(`/sessions/${encodeURIComponent(sessionId)}/decisions`, {
      method: "POST",
      body: JSON.stringify(decision),
    });
  }

  async pack(sessionId: string, profile: PackProfile = "generic"): Promise<ContextPack> {
    return this.request<ContextPack>(`/sessions/${encodeURIComponent(sessionId)}/pack`, {
      method: "POST",
      body: JSON.stringify({ profile }),
    });
  }

  async registerAdapter(descriptor: CapabilityDescriptor): Promise<unknown> {
    return this.request<unknown>("/adapters/register", {
      method: "POST",
      body: JSON.stringify({ metadata: {}, ...descriptor }),
    });
  }

  async *events(sessionId?: string): AsyncGenerator<BusEvent> {
    const query = sessionId ? `?session_id=${encodeURIComponent(sessionId)}` : "";
    const response = await fetch(`${this.baseUrl}/events${query}`, {
      headers: { accept: "application/x-ndjson" },
    });
    if (!response.ok) {
      throw new Error(`Sessionbus returned ${response.status}: ${await response.text()}`);
    }
    if (!response.body) {
      return;
    }
    yield* parseNdjsonStream<BusEvent>(response.body);
  }

  private async request<T>(path: string, init: RequestInit): Promise<T> {
    const response = await fetch(`${this.baseUrl}${path}`, {
      ...init,
      headers: {
        "content-type": "application/json",
        accept: "application/json",
        ...init.headers,
      },
    });
    const body = await response.text();
    if (!response.ok) {
      throw new Error(`Sessionbus returned ${response.status}: ${body}`);
    }
    return JSON.parse(body) as T;
  }
}

export async function* parseNdjsonStream<T>(
  stream: ReadableStream<Uint8Array>,
): AsyncGenerator<T> {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  while (true) {
    const { value, done } = await reader.read();
    if (done) {
      break;
    }
    buffer += decoder.decode(value, { stream: true });
    let newlineIndex = buffer.indexOf("\n");
    while (newlineIndex >= 0) {
      const line = buffer.slice(0, newlineIndex).trim();
      buffer = buffer.slice(newlineIndex + 1);
      if (line) {
        yield JSON.parse(line) as T;
      }
      newlineIndex = buffer.indexOf("\n");
    }
  }

  buffer += decoder.decode();
  const finalLine = buffer.trim();
  if (finalLine) {
    yield JSON.parse(finalLine) as T;
  }
}

export function createAdapterDescriptor(input: CapabilityDescriptor): CapabilityDescriptor {
  return {
    metadata: {},
    ...input,
  };
}
