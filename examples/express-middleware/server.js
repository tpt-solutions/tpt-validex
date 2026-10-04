// Express middleware validating bodies with tpt-validex (WASM).
const express = require("express");
const { Validator } = require("../tpt-valid-wasm/pkg/tpt_valid_wasm.js");

const ORDER_SCHEMA = {
  type: "object",
  properties: {
    sku: { type: "string", minLength: 3 },
    qty: { type: "integer", minimum: 1 },
  },
  required: ["sku", "qty"],
  additionalProperties: false,
};

const validator = new Validator(ORDER_SCHEMA);

/** Express middleware: 422 with structured errors when the body fails. */
function validateBody(validator) {
  return (req, res, next) => {
    const { isValid, errors } = validator.validate(req.body ?? null);
    if (!isValid) {
      return res.status(422).json({ errors });
    }
    next();
  };
}

const app = express();
app.use(express.json());
app.post("/orders", validateBody(validator), (req, res) => {
  res.json({ order: req.body, status: "accepted" });
});

app.listen(3000, () => console.log("listening on :3000"));
