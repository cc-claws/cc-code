// Client → Server 消息类型
export interface RequestPairMessage {
  type: "request_pair";
}

export interface JoinPairMessage {
  type: "join_pair";
  pair_code: string;
}

export interface SyncConfigMessage {
  type: "sync_config";
  payload: unknown;
}

export interface DataChunkMessage {
  type: "data_chunk";
  seq: number;
  data: number[]; // JSON 序列化后为 number[]
}

export interface TransferCompleteMessage {
  type: "transfer_complete";
  checksum: string;
}

// Server → Client 消息类型
export interface PairCreatedMessage {
  type: "pair_created";
  pair_code: string;
}

export interface PairJoinedMessage {
  type: "pair_joined";
  peer_info?: string;
}

export interface ErrorMessage {
  type: "error";
  code: string;
  message: string;
}

// 联合类型
export type WsClientMessage =
  | RequestPairMessage
  | JoinPairMessage
  | SyncConfigMessage
  | DataChunkMessage
  | TransferCompleteMessage;

export type WsServerMessage =
  | PairCreatedMessage
  | PairJoinedMessage
  | DataChunkMessage
  | TransferCompleteMessage
  | ErrorMessage;
