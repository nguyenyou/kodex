package com.example

// JS-only code. Should appear once in kodex index under the js module.

class JsService extends SharedService {
  def validate(input: String): Either[AppError, String] =
    SharedService.defaultValidation(input)

  def process(input: String): String =
    s"JS processed: $input"
}
