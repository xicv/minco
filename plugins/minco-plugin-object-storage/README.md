# minco-plugin-object-storage

Provider-neutral object storage for Minco plugins and applications. The crate
ships an in-memory conformance implementation for tests and local development;
production applications inject an S3, filesystem, or other adapter through the
same `ObjectStore` port.
