import { Currency, Rarity } from "../../model";

export function handleAddRarity(
  rarities: Map<string, Rarity>,
  name: string,
  price: bigint,
  maxSupply: bigint,
  currency: Currency
): void {
  let rarity = rarities.get(name);

  if (!rarity) {
    rarity = new Rarity({ id: name });
  } else if (rarity.currency !== currency) {
    console.log(
      "ERROR: Ignoring because it was not added with the current Rarity Contract"
    );
    return;
  }

  rarity.name = name;
  rarity.price = price;
  rarity.maxSupply = maxSupply;
  rarity.currency = currency;
  rarities.set(name, rarity);
}

export function handleUpdatePrice(
  rarities: Map<string, Rarity>,
  name: string,
  price: bigint,
  currency: string
): void {
  const rarity = rarities.get(name);

  if (!rarity) {
    console.log(`ERROR: Rarity with name ${name} not found`);
    return;
  }

  if (rarity.currency !== currency) {
    console.log(
      "ERROR: Ignoring because it was not added with the current Rarity Contract"
    );
    return;
  }

  rarity.price = price;
  rarity.currency = currency;
  rarities.set(name, rarity);
}
