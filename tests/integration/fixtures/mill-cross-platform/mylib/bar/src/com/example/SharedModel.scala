package com.example

// Shared code compiled by both JVM and JS targets.
// kodex should index this only once, not twice.

sealed trait AppError {
  def message: String
}

object AppError {
  final case class NotFound(id: String) extends AppError {
    def message: String = s"Not found: $id"
  }

  final case class Forbidden(reason: String) extends AppError {
    def message: String = s"Forbidden: $reason"
  }

  final case class ValidationFailed(fields: List[String]) extends AppError {
    def message: String = s"Validation failed: ${fields.mkString(", ")}"
  }
}
