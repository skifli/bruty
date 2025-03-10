# Bruty Server and Client Documentation

## 1. Endpoints

- **WebSocket Endpoint**: `/ivocord`
  - **Method**: `GET`.
  - **Purpose**: Establishes a WebSocket connection for real-time communication.
  - **Authentication**: Clients must identify themselves using the `Identify` OP code after connecting.
- **Status Endpoint**: `/status`
  - **Method**: `GET`.
  - **Purpose**: Returns the server's status (used by the client to check if the server is online).

## 2. WebSocket Communication Overview

The Bruty server and client communicate over a **WebSocket** connection. The server exposes a WebSocket **endpoint** (`/ivocord`) that **clients** connect to. Once connected, the client and server exchange **binary messages** serialized using `rmp_serde`. These messages contain **payloads** that define the operation to be *performed* (via **OP Codes**) and the *associated* **data**.

The **WebSocket connection** is *persistent* and *bidirectional*, allowing both the server and client to *send* and *receive* messages in *real-time*.

## 3. Payload Structure

All **messages** exchanged between the **server** and **client** are *encapsulated* in a `Payload` struct. This struct contains *two* main **fields**:
- **`op_code`**: The **OP code** *hat *defines* the *type* of **action** to be *performed*.
- **`data`**: The **data** *associated* with the operation, which *varies* depending on the **OP code**.

Here is the `Payload` struct definition:

```rust
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Payload {
    pub op_code: OperationCode,
    pub data: Data,
}
```

The `OperationCode` enum defines the supported operations:

```rust
#[derive(serde::Serialize, serde::Deserialize)]
pub enum OperationCode {
    Heartbeat,          // Keeps the connection alive
    Identify,           // Authenticates the client
    TestRequestData,    // Requests the client to test a specific ID
    TestingResult,      // Sends test results to the server
    InvalidSession,     // Indicates a session-related error
}
```

The `Data` enum defines the data associated with each operation:

```rust
#[derive(serde::Serialize, serde::Deserialize)]
pub enum Data {
    Heartbeat,                          // No additional data
    Identify(IdentifyData),             // Contains client credentials
    TestRequestData(TestRequestData),   // Contains the ID to be tested
    TestingResult(TestingResultData),   // Contains test results
    InvalidSession(InvalidSessionData), // Contains error information
}
```

## 4. Interaction Flow

### Step 1: Client Connects to the Server
- The **client** establishes a **WebSocket connection** to the server's `/ivocord` **endpoint**.
- The **server** accepts the **connection** and *waits* for the **client** to *authenticate*.

### Step 2: Client Sends Identify Payload
- The **client** sends an `Identify` **payload** to *authenticate* itself. This payload *contains*:
  - **Client Version**: The *version* of the **client**.
  - **ID**: The client's unique *identifier*.
  - **Secret**: The client's *secret* for **authentication**.

Example `Identify` payload:

```rust
Payload {
    op_code: OperationCode::Identify,
    data: Data::Identify(IdentifyData {
        client_version: "0.0.1".to_string(),
        id: 1,
        secret: "client_secret".to_string(),
    }),
}
```

### Step 3: Server Authenticates the Client
- The server *verifies* the client's **credentials**.
- If **authentication** *succeeds*, the server marks the **session** as *authenticated* and proceeds to send *test* **requests**.
- If **authentication** *fails*, the server sends an `InvalidSession` payload with an *appropriate* **error code** and *closes* the **connection**.

Example `InvalidSession` payload:

```rust
Payload {
    op_code: OperationCode::InvalidSession,
    data: Data::InvalidSession(InvalidSessionData {
        code: ErrorCode::AuthenticationFailed,
        description: "Authentication failed".to_string(),
        explanation: "The server received an invalid passphrase.".to_string(),
    }),
}
```

### Step 4: Server Sends TestRequestData Payload
- Once **authenticated**, the server sends a `TestRequestData` payload to the client. This payload contains:
  - **ID**: The **base ID** to be *tested*.

Example `TestRequestData` payload:

```rust
Payload {
    op_code: OperationCode::TestRequestData,
    data: Data::TestRequestData(TestRequestData {
        id: vec!['a', 'b', 'c'],
    }),
}
```

### Step 5: Client Generates and Tests IDs
- The **client** generates *permutations* of the **base ID** and *tests* them using the **YouTube API**.
- For each ID, the client makes an **HTTP request** to the **YouTube API** and *checks* the **response**.
- The client *collects* only the *positive* **results** and sends them *back* to the **server**.

### Step 6: Client Sends TestingResult Payload
- The **client** sends a `TestingResult` payload to the **server**. This **payload** *contains*:
  - **ID**: The **base ID** that was *tested*.
  - **Positives**: A list of **positive results** (*valid* **YouTube video IDs**).

Example `TestingResult` payload:

```rust
Payload {
    op_code: OperationCode::TestingResult,
    data: Data::TestingResult(TestingResultData {
        id: vec!['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'],
        positives: vec![
            Video {
                event: VideoEvent::Success,
                id: vec!['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k'],
                video_data: Some(VideoData {
                    title: "Example Video".to_string(),
                    author_name: "Example Author".to_string(),
                    author_url: "https://example.com".to_string(),
                }),
            },
        ],
    }),
}
```

### Step 7: Server Processes Results
- The **server** *processes* the **results** and updates its **internal state**.
- If the results are *valid*, the **server** sends a *new* `TestRequestData` payload to the **client** for *further* **testing**.
- If the results are *invalid* or an error occurs, the server sends an `InvalidSession` **payload** and *closes* the **connection**.

## 5. Heartbeat Mechanism

To keep the **WebSocket connection** *alive*, the **client** sends a `Heartbeat` payload to the server every `5` seconds. The server expects to receive a **heartbeat** within `10` seconds (to *accommodate* for **latency**). If no **heartbeat** is *received*, the server assumes the **connection** is *dead* and *closes* it.

Example `Heartbeat` payload:

```rust
Payload {
    op_code: OperationCode::Heartbeat,
    data: Data::Heartbeat,
}
```

## 6. **Error Handling**

Errors are communicated using the `InvalidSession` payload. This payload contains:
- **Error Code**: The *type* of **error**.
- **Description**: A *brief* **description** of the error.
- **Explanation**: A *detailed* **explanation** of the error.

Example `InvalidSession` payload for a session timeout:

```rust
Payload {
    op_code: OperationCode::InvalidSession,
    data: Data::InvalidSession(InvalidSessionData {
        code: ErrorCode::SessionTimeout,
        description: "Session timeout".to_string(),
        explanation: "You didn't send a heartbeat in time.".to_string(),
    }),
}
```

## 7. Thread Management

### Server Threads
- **Permutation Generator**: Generates **permutations** of IDs for *testing*.
- **Results Progress Handler**: Tracks the **progress** of ID *testing*.
- **Results Handler**: Processes *positive* **results** from **clients**.

### Client Threads
- **ID Generator**: Generates **permutations** of **IDs** based on the *base* **ID**.
- **ID Checker**: Validates **IDs** using the **YouTube API**.

## 8. Combined Workflow

1. The **client** *connects* to the server and *authenticates*.
2. The **server** sends a `TestRequestData` payload to the **client**.
3. The **client** *generates* and *tests* IDs, sending the **results** back to the **server**.
4. The **server** *processes* the *results* and sends a new `TestRequestData` **payload**.
5. The **client** and **server** continue this *loop* until all **IDs** are *tested* or an **error** occurs.

## Conclusion

The Bruty server and client interact over a WebSocket connection, exchanging payloads that define the operations to be performed. The server manages the testing process, while the client generates and validates IDs. The use of OP codes and structured payloads ensures a clear and efficient communication protocol. This architecture allows for scalable and real-time brute-forcing of YouTube video IDs.