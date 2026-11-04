FeatureScript 2500;
import(path : "onshape/std/geometry.fs", version : "2500.0");
annotation { "Feature Type Name" : "Bounded cuboid" }
export const boundedCuboid = defineFeature(function(context is Context, id is Id, definition is map)
    precondition
    {
        annotation { "Name" : "Length" }
        isLength(definition.length, NONNEGATIVE_LENGTH_BOUNDS);
        annotation { "Name" : "Width" }
        isLength(definition.width, NONNEGATIVE_LENGTH_BOUNDS);
        annotation { "Name" : "Height" }
        isLength(definition.height, NONNEGATIVE_LENGTH_BOUNDS);
    }
    {
        fCuboid(context, id + "cuboid", {
            "corner1" : vector(0, 0, 0) * millimeter,
            "corner2" : vector(definition.length, definition.width, definition.height)
        });
    });
