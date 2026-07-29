fn main() {
	tonic_build::configure()
		.compile_protos(&["src/grpc/proto/midnight_indexer.proto"], &["src/grpc/proto"])
		.unwrap();
}
