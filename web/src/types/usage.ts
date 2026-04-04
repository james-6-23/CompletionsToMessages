export interface UsageSummary {
  total_requests: number;
  total_cost: string;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_creation_tokens: number;
  total_cache_read_tokens: number;
}

export interface DailyStats {
  date: string;
  request_count: number;
  total_cost: string;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_creation_tokens: number;
  total_cache_read_tokens: number;
}

export interface RequestLog {
  request_id: string;
  model: string;
  request_model: string | null;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  total_cost_usd: string;
  latency_ms: number;
  first_token_ms: number | null;
  status_code: number;
  is_streaming: boolean;
  error_message: string | null;
  channel_id: string;
  created_at: number;
}

export interface PaginatedLogs {
  data: RequestLog[];
  total: number;
  page: number;
  page_size: number;
}

export interface ModelStats {
  model: string;
  request_count: number;
  total_cost: string;
  input_tokens: number;
  output_tokens: number;
  cache_creation_tokens: number;
  cache_read_tokens: number;
  avg_latency_ms: number;
}

export interface ModelPricing {
  model_id: string;
  display_name: string;
  input_cost_per_million: string;
  output_cost_per_million: string;
  cache_read_cost_per_million: string;
  cache_creation_cost_per_million: string;
}

export type TimeRange = "1h" | "6h" | "1d" | "7d" | "30d";

export interface ApiKey {
  id: string;
  endpoint_id: string;
  api_key_masked: string;
  label: string;
  is_active: boolean;
  total_requests: number;
  failed_requests: number;
  last_used_at: number | null;
  created_at: number;
}

export interface Endpoint {
  id: string;
  name: string;
  base_url: string;
  is_active: boolean;
  key_count: number;
  created_at: number;
}

export interface TestKeyResult {
  valid: boolean;
  status?: number;
  error?: string;
}

export interface AccessToken {
  id: string;
  token_masked: string;
  name: string;
  is_active: boolean;
  total_requests: number;
  failed_requests: number;
  last_used_at: number | null;
  channel_ids: string[];
  created_at: number;
}

export interface AccessTokenCreated extends AccessToken {
  token: string;
}
