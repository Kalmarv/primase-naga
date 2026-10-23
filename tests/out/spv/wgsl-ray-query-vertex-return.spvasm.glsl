#version 460
#extension GL_EXT_ray_query : require
#extension GL_EXT_ray_tracing_position_fetch : require
layout(local_size_x = 1, local_size_y = 1, local_size_z = 1) in;

struct _9
{
    vec3 _m0[3];
    vec3 _m1[3];
};

struct _11
{
    uint _m0;
    uint _m1;
    float _m2;
    float _m3;
    vec3 _m4;
    vec3 _m5;
};

layout(set = 0, binding = 1, std430) buffer _15_14
{
    _9 _m0;
} _14;

layout(set = 0, binding = 0) uniform accelerationStructureEXT _12;

void _41(rayQueryEXT _42, accelerationStructureEXT _43, _11 _44, inout uint _45, out float _46)
{
    bool _72 = (_44._m0 & 256u) != 0u;
    bool _80 = (_44._m0 & 16u) != 0u;
    bool _83 = (_44._m0 & 32u) != 0u;
    bool _91 = (_44._m0 & 1u) != 0u;
    bool _94 = (_44._m0 & 2u) != 0u;
    bool _97 = (_44._m0 & 64u) != 0u;
    bool _100 = (_44._m0 & 128u) != 0u;
    _46 = _44._m3;
    if (((((((!((((((_94 && _91) || (_100 && _91)) || (_100 && _94)) || (_100 && _97)) || (_97 && _91)) || (_97 && _94))) && (_44._m2 <= _44._m3)) && (_44._m2 >= 0.0)) && (!(any(isnan(_44._m4)) || any(isinf(_44._m4))))) && (!(any(isnan(_44._m5)) || any(isinf(_44._m5))))) && (!(((_44._m0 & 512u) != 0u) && _72))) && (!(((_80 && _72) || (_83 && _72)) || (_83 && _80))))
    {
        rayQueryInitializeEXT(_42, _43, _44._m0, _44._m1, _44._m4, _44._m2, _44._m5, _44._m3);
        _45 = 1u;
    }
    else
    {
    }
}

bool _147(rayQueryEXT _148, inout uint _149)
{
    bool _151 = false;
    if ((!((_149 & 4u) != 0u)) && ((_149 & 1u) != 0u))
    {
        bool _163 = rayQueryProceedEXT(_148);
        _151 = _163;
        _149 |= (_163 ? 2u : 6u);
    }
    return _151;
}

vec3[3] _175(rayQueryEXT _176, uint _177)
{
    vec3 _179[3] = vec3[](vec3(0.0), vec3(0.0), vec3(0.0));
    if ((!((_177 & 4u) != 0u)) && ((_177 & 2u) != 0u))
    {
        uint _190 = rayQueryGetIntersectionTypeEXT(_176, bool(0u));
        if (_190 == 0u)
        {
            vec3 _194[3];
            rayQueryGetIntersectionTriangleVertexPositionsEXT(_176, bool(0u), _194);
            _179 = _194;
        }
    }
    return _179;
}

void _199(rayQueryEXT _200, uint _201)
{
    if ((!((_201 & 4u) != 0u)) && ((_201 & 2u) != 0u))
    {
        uint _212 = rayQueryGetIntersectionTypeEXT(_200, bool(0u));
        if (_212 == 0u)
        {
            rayQueryConfirmIntersectionEXT(_200);
        }
    }
}

vec3[3] _217(rayQueryEXT _218, uint _219)
{
    vec3 _221[3] = vec3[](vec3(0.0), vec3(0.0), vec3(0.0));
    if (((_219 & 4u) != 0u) && ((_219 & 2u) != 0u))
    {
        uint _230 = rayQueryGetIntersectionTypeEXT(_218, bool(1u));
        if (_230 == 1u)
        {
            vec3 _234[3];
            rayQueryGetIntersectionTriangleVertexPositionsEXT(_218, bool(1u), _234);
            _221 = _234;
        }
    }
    return _221;
}

void main()
{
    uint _36 = 0u;
    float _38 = 0.0;
    uvec2 _133 = uvec2(4294967295u);
    rayQueryEXT _33;
    _41(_33, _12, _11(1u, 255u, 0.100000001490116119384765625, 100.0, vec3(0.0), vec3(0.0, 1.0, 0.0)), _36, _38);
    for (;;)
    {
        if (all(equal(uvec2(0u), _133)))
        {
            break;
        }
        _133 -= uvec2(uint(_133.y == 0u), 1u);
        bool _144 = _147(_33, _36);
        if (!_144)
        {
            break;
        }
        _14._m0._m1 = _175(_33, _36);
        _199(_33, _36);
        continue;
    }
    _14._m0._m0 = _217(_33, _36);
}

