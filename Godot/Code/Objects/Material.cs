using Godot;
using System;

namespace Deuteros.Code.Objects
{
    [Serializable]
    public partial class Material
    {
        //Total amount of resources available in current vein
        public int GroundAmount { get; set; }
        public Enums.ItemTypes MaterialType { get; set; }
        //Maintains a record of how long an active survey has been on-going
        public int SurveyTicks { get; set; }

        public Material(Enums.ItemTypes materialType, int surveyTicks)
        {
            MaterialType = materialType;
            SurveyTicks = surveyTicks;
            GroundAmount = 0;
        }
    }
}