请使用 subagent say hello

> 验证 bg agent 消息回调触发会话

请使用 bg hello subagent  say hello， 但是它要先 sleep 10s

> 验证 agent 树状聚合

请直接派出三个同步非 bg 的 hello agent say hello

> 验证 bg fork agent 消息回调触发会话

请使用 bg fork subagent  say hello， 但是它要先 sleep 10s

> 验证 bg agent 消息并发回调触发会话

请直接派出三个 bg 的 hello agent say hello
