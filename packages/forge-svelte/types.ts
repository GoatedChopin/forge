
/** Wire-format error from the Forge RPC layer. */
export interface ForgeError {
  code: string;
  message: string;
  /** Seconds to wait before retrying. Present on RATE_LIMITED errors. */
  retry_after_secs?: number;
  details?: Record<string, unknown>;
}

export interface QueryResult<T> {
  loading: boolean;
  data: T | null;
  error: ForgeError | null;
}

export interface SubscriptionResult<T> extends QueryResult<T> {
  stale: boolean;
}

export type ConnectionState = "disconnected" | "connecting" | "connected";

export interface AuthState {
  user: unknown | null;
  token: string | null;
  loading: boolean;
}

export type JobStatus =
  | "pending"
  | "claimed"
  | "running"
  | "completed"
  | "retry"
  | "failed"
  | "dead_letter"
  | "cancel_requested"
  | "cancelled";

export interface JobState<TOutput = unknown> {
  jobId: string;
  status: JobStatus;
  progress: number | null;
  message: string | null;
  output: TOutput | null;
  error: string | null;
}

export type WorkflowStatus =
  | "created"
  | "running"
  | "waiting"
  | "completed"
  | "compensating"
  | "compensated"
  | "failed"
  | "blocked_missing_version"
  | "blocked_signature_mismatch"
  | "blocked_missing_handler"
  | "retired_unresumable"
  | "cancelled_by_operator";

export interface WorkflowStepState {
  name: string;
  status: "pending" | "running" | "completed" | "failed" | "compensated" | "skipped";
  error: string | null;
}

export interface WorkflowState<TOutput = unknown> {
  workflowId: string;
  status: WorkflowStatus;
  step: string | null;
  waitingFor: string | null;
  steps: WorkflowStepState[];
  output: TOutput | null;
  error: string | null;
}

/**
 * Reactive wrapper around a one-shot mutation call. Mirrors the shape produced
 * by the generated `toReactiveMutation()` helper so user components can type
 * their `mutationFn$()` results without reaching into the generated module.
 */
export interface ReactiveMutation<TArgs, TResult> {
  mutate: (args: TArgs) => Promise<TResult>;
  pending: boolean;
  error: ForgeError | null;
}
