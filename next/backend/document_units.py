"""Document-local presentation. All unit parsing and conversion is FreeCAD's.

This module never changes Units preferences or creates a native document.
Numeric protocol values keep their existing base-unit meanings.
"""
import copy
import math
import FreeCAD as App


class QuantityError(ValueError):
    pass


def finite(value):
    if type(value) not in (int, float) or not math.isfinite(value):
        raise QuantityError('Expected a finite quantity value')
    return value


def native(value, unit):
    # Multiplication by a native quantity retains scale, including rad -> deg.
    return App.Units.Quantity(finite(value)) * App.Units.Quantity('1 ' + unit)


def index(doc):
    return list(doc.getEnumerationsOfProperty('UnitSystem')).index(doc.UnitSystem)


def describe(quantity, schema):
    display, _, default = App.Units.schemaTranslate(quantity, schema)
    # schemaTranslate's text is deliberately rounded for display. Never send it
    # back as a geometry value. Keep a separate lossless edit seed.
    converted = quantity.getValueAs(default) if default else App.Units.Quantity(quantity.Value)
    finite(converted.Value)
    # Format the converted, dimensionless number natively. Formatting the whole
    # length again would reselect units or round Building-US fractions. General
    # precision preserves very small values as well as ordinary edit seeds.
    converted.Format = {'Precision': 17, 'NumberFormat': 'g'}
    text = App.Units.schemaTranslate(converted, schema)[0]
    return dict(display=display, input=text + (' ' + default if default else ''),
                default_unit=default, signature=list(quantity.Unit.Signature), value=quantity.Value)


def parse(text, unit, default=None):
    if not isinstance(text, str) or not text.strip():
        raise QuantityError('Enter a native quantity')
    expected = App.Units.Quantity('1 ' + unit).Unit
    result = App.Units.Quantity(text)
    if result.Unit == App.Units.Unit('') and expected != App.Units.Unit(''):
        if not default:
            raise QuantityError('A physical quantity needs a unit')
        # The UI provides the native field's default unit, not a conversion.
        result = native(result.Value, default)
    if result.Unit != expected:
        raise QuantityError('Quantity dimension does not match ' + (unit or 'a unitless value'))
    finite(result.Value)
    converted = result.getValueAs(unit) if unit else result.Value
    return finite(converted.Value if hasattr(converted, 'Value') else float(converted))


def value(value, unit):
    if isinstance(value, dict) and 'quantity_input' in value:
        return parse(value['quantity_input'], unit, value.get('default_unit'))
    return parse(value, unit) if isinstance(value, str) else finite(value)


def property_input(value, expected):
    if not isinstance(value, dict) or set(value) != {'quantity_input', 'default_unit'}:
        raise QuantityError('Expected native quantity input and default unit')
    if not isinstance(value["quantity_input"], str) or not value["quantity_input"].strip():
        raise QuantityError("Enter a native quantity")
    q = App.Units.Quantity(value['quantity_input'])
    if q.Unit == App.Units.Unit('') and expected != q.Unit:
        q = native(q.Value, value['default_unit'])
    if q.Unit != expected:
        raise QuantityError('Quantity dimension does not match the native property')
    finite(q.Value)
    return q.Value


def metadata(type_id, value, schema):
    if hasattr(value, 'Unit') and hasattr(value, 'Value'):
        return describe(value, schema)
    if type_id in {'App::PropertyPosition', 'App::PropertyVectorDistance'}:
        return [describe(native(v, 'mm'), schema) for v in (value.x, value.y, value.z)]
    if type_id == 'App::PropertyPlacement':
        return dict(position=[describe(native(v, 'mm'), schema) for v in (value.Base.x, value.Base.y, value.Base.z)])
    if type_id == 'Part::PropertyFilletEdges':
        return [[None, describe(native(row[1], 'mm'), schema), describe(native(row[2], 'mm'), schema)] for row in value]
    return None


def reformat(item, schema):
    if isinstance(item, dict) and 'signature' in item:
        return describe(App.Units.Quantity(item['value'], App.Units.Unit(*item['signature'])), schema)
    if isinstance(item, dict):
        return {k: reformat(v, schema) for k, v in item.items()}
    if isinstance(item, list):
        return [reformat(v, schema) for v in item]
    return item


def schema(cached, doc, values):
    result = copy.deepcopy(cached)
    selected = index(doc)
    for prop, meta in result['property_metadata'].items():
        if 'quantity_default' in meta:
            meta['quantity_default'] = reformat(meta['quantity_default'], selected)
        if 'quantity' in meta:
            meta['quantity'] = reformat(meta['quantity'], selected)
            if prop in values and isinstance(meta['quantity'], dict) and 'signature' in meta['quantity']:
                q = App.Units.Quantity(values[prop]) if isinstance(values[prop], str) else App.Units.Quantity(
                    finite(values[prop]), App.Units.Unit(*meta['quantity']['signature']))
                if q.Unit != App.Units.Unit(*meta['quantity']['signature']):
                    raise QuantityError('Schema default has the wrong dimension: ' + prop)
                meta['quantity'] = dict(source=values[prop], **describe(q, selected))
    return result


def settings(doc):
    selected = index(doc)
    return dict(index=selected, label=doc.UnitSystem,
                choices=[dict(index=i, label=label) for i, label in enumerate(doc.getEnumerationsOfProperty('UnitSystem'))],
                defaults={unit: describe(native(0, unit), selected) for unit in ('mm', 'deg', 'rad', 'mm^2', 'mm^3')},
                undoable=False)


def constraint(obj, c):
    kind = c.Type
    if kind not in {'Distance', 'DistanceX', 'DistanceY', 'Radius', 'Diameter', 'Angle', 'Weight', 'SnellsLaw'}:
        return {}
    unit = 'rad' if kind == 'Angle' else '' if kind in {'Weight', 'SnellsLaw'} else 'mm'
    return dict(quantity=describe(native(c.Value, unit), index(obj.Document)))


def query(doc, items):
    if not isinstance(items, list) or len(items) > 256:
        raise QuantityError('Expected up to 256 quantity queries')
    results = []
    for item in items:
        unit = item['unit']
        if not isinstance(unit, str):
            raise QuantityError('Expected a native unit string')
        base = parse(item['text'], unit, item.get('default_unit')) if 'text' in item else finite(item['value'])
        results.append(dict(base_value=base, source=item.get("text", item.get("value")), **describe(native(base, unit), index(doc))))
    return results
