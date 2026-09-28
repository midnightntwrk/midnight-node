def bytes: {bytes: ltrimstr("0x")};
def constr($fields): {constructor: 0, fields: $fields};
($signatures[0]) as $s | ($keys[0]) as $k | ($utxo | split("#")) as $u |
{list: [
  ($owner | bytes),
  constr([
    constr([($s.spo_public_key | bytes), ($s.spo_signature | bytes)]),
    ($s.sidechain_public_key | bytes),
    ($s.sidechain_signature | bytes),
    constr([constr([($u[0] | bytes)]), {int: ($u[1] | tonumber)}]),
    {list: [
      {list: [{bytes: "61757261"}, ($k.aura | bytes)]},
      {list: [{bytes: "6772616e"}, ($k.grandpa | bytes)]},
      {list: [{bytes: "62656566"}, ($k.beefy | bytes)]}
    ]}
  ]),
  {int: 1}
]}
