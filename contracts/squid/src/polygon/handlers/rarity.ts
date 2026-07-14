import { Currency, Rarity } from "../../model";
import { AddRarityEventArgs, UpdatePriceEventArgs } from "../abi/Rarity";
import * as utils from "../modules/rarity";

export function handleAddRarity(
  rarities: Map<string, Rarity>,
  event: AddRarityEventArgs,
  currency: Currency
): void {
  utils.handleAddRarity(
    rarities,
    event._rarity.name,
    event._rarity.price,
    event._rarity.maxSupply,
    currency
  );
}

export function handleUpdatePrice(
  rarities: Map<string, Rarity>,
  event: UpdatePriceEventArgs,
  currency: Currency
): void {
  utils.handleUpdatePrice(rarities, event._name, event._price, currency);
}
