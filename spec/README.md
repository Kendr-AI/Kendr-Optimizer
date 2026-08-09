# Contract specifications

The JSON schemas describe the language-neutral wire format. Rust APIs may evolve
more quickly, but an incompatible wire change requires a new schema version.

- envelope-v1.schema.json validates optimization requests.
- receipt-v1.schema.json validates the receipt portion of a response.

Provider-specific adapters should preserve unknown provider fields outside this
contract, map only supported content into the envelope, and use the returned
complete envelope as the authoritative transformed value.

