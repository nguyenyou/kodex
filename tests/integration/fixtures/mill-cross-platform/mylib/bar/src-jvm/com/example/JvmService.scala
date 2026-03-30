package com.example

// JVM-only code. Should appear once in kodex index under the jvm module.

class JvmService extends SharedService {
  def validate(input: String): Either[AppError, String] =
    SharedService.defaultValidation(input)

  def process(input: String): String =
    s"JVM processed: $input"
}
