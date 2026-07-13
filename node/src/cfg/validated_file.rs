// This file is part of midnight-node.
// Copyright (C) 2025-2026 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::fs;
use std::path::Path;

/// Maximum allowed file size for safe reads (10 MB).
pub const DEFAULT_SAFE_READ_FILE_SIZE: u64 = 10 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct SafeReadOpts {
	/// Maximum allowed file size when reading
	pub max_size: u64,
	/// Allow Symlinks, accepting associated symlink-attack risks
	pub unsafe_allow_symlinks: bool,
}

impl Default for SafeReadOpts {
	fn default() -> Self {
		Self { max_size: DEFAULT_SAFE_READ_FILE_SIZE, unsafe_allow_symlinks: false }
	}
}

fn validate_file_metadata(path: &Path, opts: &SafeReadOpts) -> Result<u64, String> {
	let meta = fs::symlink_metadata(path).map_err(|e| {
		format!("failed to read metadata (not following symlinks) for '{}': {e}", path.display())
	})?;

	if !opts.unsafe_allow_symlinks && meta.file_type().is_symlink() {
		return Err(format!("'{}' is a symlink; symlinks are not allowed", path.display()));
	}

	let meta = fs::metadata(path).map_err(|e| {
		format!("failed to read metadata (following symlinks) for '{}': {e}", path.display())
	})?;

	if !meta.file_type().is_file() {
		return Err(format!("'{}' is not a regular file", path.display()));
	}

	Ok(meta.len())
}

/// Read a file as bytes after validating it is a regular file within the size limit.
pub fn safe_read(path: &str, opts: &SafeReadOpts) -> Result<Vec<u8>, String> {
	let p = Path::new(path);
	let size = validate_file_metadata(p, opts)?;

	if size > opts.max_size {
		return Err(format!(
			"'{}' exceeds maximum allowed size ({size} bytes > {} bytes)",
			p.display(),
			opts.max_size
		));
	}

	fs::read(p).map_err(|e| format!("failed to read '{}': {e}", p.display()))
}

/// Read a file as a UTF-8 string after validating it is a regular file within the size limit.
pub fn safe_read_to_string(path: &str, opts: &SafeReadOpts) -> Result<String, String> {
	let p = Path::new(path);
	let size = validate_file_metadata(p, opts)?;

	if size > opts.max_size {
		return Err(format!(
			"'{}' exceeds maximum allowed size ({size} bytes > {} bytes)",
			p.display(),
			opts.max_size
		));
	}

	fs::read_to_string(p).map_err(|e| format!("failed to read '{}': {e}", p.display()))
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Write;

	#[test]
	fn safe_read_succeeds_for_regular_file() {
		let mut f = tempfile::NamedTempFile::new().unwrap();
		f.write_all(b"hello world").unwrap();
		let result = safe_read(f.path().to_str().unwrap(), &Default::default());
		assert_eq!(result.unwrap(), b"hello world");
	}

	#[test]
	fn safe_read_to_string_succeeds_for_regular_file() {
		let mut f = tempfile::NamedTempFile::new().unwrap();
		f.write_all(b"hello world").unwrap();
		let result = safe_read_to_string(f.path().to_str().unwrap(), &Default::default());
		assert_eq!(result.unwrap(), "hello world");
	}

	#[cfg(unix)]
	#[test]
	fn safe_read_rejects_symlink() {
		use std::os::unix::fs::symlink;

		let f = tempfile::NamedTempFile::new().unwrap();
		let dir = tempfile::tempdir().unwrap();
		let link_path = dir.path().join("link");
		symlink(f.path(), &link_path).unwrap();

		let result = safe_read(link_path.to_str().unwrap(), &Default::default());
		let err = result.unwrap_err();
		assert!(err.contains("symlink"), "expected 'symlink' in error: {err}");
	}

	#[cfg(unix)]
	#[test]
	fn safe_read_to_string_rejects_symlink() {
		use std::os::unix::fs::symlink;

		let f = tempfile::NamedTempFile::new().unwrap();
		let dir = tempfile::tempdir().unwrap();
		let link_path = dir.path().join("link");
		symlink(f.path(), &link_path).unwrap();

		let result = safe_read_to_string(link_path.to_str().unwrap(), &Default::default());
		let err = result.unwrap_err();
		assert!(err.contains("symlink"), "expected 'symlink' in error: {err}");
	}

	#[cfg(unix)]
	#[test]
	fn safe_read_allows_symlink_with_opt() {
		use std::os::unix::fs::symlink;

		let mut f = tempfile::NamedTempFile::new().unwrap();
		f.write_all(b"hello world").unwrap();
		let dir = tempfile::tempdir().unwrap();
		let link_path = dir.path().join("link");
		symlink(f.path(), &link_path).unwrap();

		let result = safe_read(
			link_path.to_str().unwrap(),
			&SafeReadOpts { unsafe_allow_symlinks: true, ..Default::default() },
		);
		assert_eq!(result.unwrap(), b"hello world");
	}

	#[cfg(unix)]
	#[test]
	fn safe_read_to_string_allows_symlink_with_opt() {
		use std::os::unix::fs::symlink;

		let mut f = tempfile::NamedTempFile::new().unwrap();
		f.write_all(b"hello world").unwrap();
		let dir = tempfile::tempdir().unwrap();
		let link_path = dir.path().join("link");
		symlink(f.path(), &link_path).unwrap();

		let result = safe_read_to_string(
			link_path.to_str().unwrap(),
			&SafeReadOpts { unsafe_allow_symlinks: true, ..Default::default() },
		);
		assert_eq!(result.unwrap(), "hello world");
	}

	#[cfg(unix)]
	#[test]
	fn safe_read_rejects_symlink_to_dir() {
		use std::os::unix::fs::symlink;

		let dir = tempfile::tempdir().unwrap();
		let dir1 = tempfile::tempdir().unwrap();

		let link_path = dir.path().join("link");
		symlink(dir1.path(), &link_path).unwrap();

		let result = safe_read(
			link_path.to_str().unwrap(),
			&SafeReadOpts { unsafe_allow_symlinks: true, ..Default::default() },
		);
		let err = result.unwrap_err();
		assert!(
			err.contains("not a regular file"),
			"expected 'not a regular file' in error: {err}"
		);
	}

	#[test]
	fn safe_read_rejects_directory() {
		let dir = tempfile::tempdir().unwrap();
		let result = safe_read(dir.path().to_str().unwrap(), &Default::default());
		let err = result.unwrap_err();
		assert!(
			err.contains("not a regular file"),
			"expected 'not a regular file' in error: {err}"
		);
	}

	#[test]
	fn safe_read_to_string_rejects_directory() {
		let dir = tempfile::tempdir().unwrap();
		let result = safe_read_to_string(dir.path().to_str().unwrap(), &Default::default());
		let err = result.unwrap_err();
		assert!(
			err.contains("not a regular file"),
			"expected 'not a regular file' in error: {err}"
		);
	}

	#[test]
	fn safe_read_rejects_oversized_file() {
		let mut f = tempfile::NamedTempFile::new().unwrap();
		f.write_all(&[0u8; 101]).unwrap();
		let result = safe_read(
			f.path().to_str().unwrap(),
			&SafeReadOpts { max_size: 100, ..Default::default() },
		);
		let err = result.unwrap_err();
		assert!(
			err.contains("exceeds maximum allowed size"),
			"expected 'exceeds maximum allowed size' in error: {err}"
		);
	}

	#[test]
	fn safe_read_to_string_rejects_oversized_file() {
		let mut f = tempfile::NamedTempFile::new().unwrap();
		f.write_all(&[b'a'; 101]).unwrap();
		let result = safe_read_to_string(
			f.path().to_str().unwrap(),
			&SafeReadOpts { max_size: 100, ..Default::default() },
		);
		let err = result.unwrap_err();
		assert!(
			err.contains("exceeds maximum allowed size"),
			"expected 'exceeds maximum allowed size' in error: {err}"
		);
	}

	#[test]
	fn safe_read_succeeds_for_empty_file() {
		let f = tempfile::NamedTempFile::new().unwrap();
		let result = safe_read(f.path().to_str().unwrap(), &Default::default());
		assert!(result.unwrap().is_empty());
	}

	#[test]
	fn safe_read_to_string_succeeds_for_empty_file() {
		let f = tempfile::NamedTempFile::new().unwrap();
		let result = safe_read_to_string(f.path().to_str().unwrap(), &Default::default());
		assert_eq!(result.unwrap(), "");
	}

	#[test]
	fn safe_read_succeeds_at_exact_size_limit() {
		let mut f = tempfile::NamedTempFile::new().unwrap();
		f.write_all(&[0u8; 100]).unwrap();
		let result = safe_read(
			f.path().to_str().unwrap(),
			&SafeReadOpts { max_size: 100, ..Default::default() },
		);
		assert_eq!(result.unwrap().len(), 100);
	}

	#[test]
	fn safe_read_to_string_succeeds_at_exact_size_limit() {
		let mut f = tempfile::NamedTempFile::new().unwrap();
		f.write_all(&[b'x'; 100]).unwrap();
		let result = safe_read_to_string(
			f.path().to_str().unwrap(),
			&SafeReadOpts { max_size: 100, ..Default::default() },
		);
		assert_eq!(result.unwrap().len(), 100);
	}

	#[test]
	fn safe_read_rejects_at_size_limit_plus_one() {
		let mut f = tempfile::NamedTempFile::new().unwrap();
		f.write_all(&[0u8; 101]).unwrap();
		let result = safe_read(
			f.path().to_str().unwrap(),
			&SafeReadOpts { max_size: 100, ..Default::default() },
		);
		assert!(result.is_err());
	}

	#[test]
	fn safe_read_returns_error_for_nonexistent_path() {
		let result = safe_read("/nonexistent/path/file.json", &Default::default());
		assert!(result.is_err());
	}

	#[test]
	fn error_message_includes_file_path() {
		let result = safe_read("/some/specific/path.json", &Default::default());
		let err = result.unwrap_err();
		assert!(err.contains("/some/specific/path.json"), "expected path in error: {err}");
	}

	#[test]
	fn error_message_includes_rejection_reason() {
		let dir = tempfile::tempdir().unwrap();
		let err = safe_read(dir.path().to_str().unwrap(), &Default::default()).unwrap_err();
		assert!(err.contains("not a regular file"), "expected rejection reason in error: {err}");

		let mut f = tempfile::NamedTempFile::new().unwrap();
		f.write_all(&[0u8; 200]).unwrap();
		let err = safe_read(
			f.path().to_str().unwrap(),
			&SafeReadOpts { max_size: 100, ..Default::default() },
		)
		.unwrap_err();
		assert!(
			err.contains("exceeds maximum allowed size"),
			"expected rejection reason in error: {err}"
		);
	}
}
