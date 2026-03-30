package com.example

// Shared service trait compiled by both JVM and JS targets.
// Tests that trait + implementations are not duplicated in kodex index.

trait SharedService {
  def validate(input: String): Either[AppError, String]
  def process(input: String): String
}

object SharedService {
  def defaultValidation(input: String): Either[AppError, String] = {
    if (input.isEmpty) Left(AppError.ValidationFailed(List("input")))
    else Right(input.trim)
  }
}
